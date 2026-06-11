//! OpenAI-compatible chat client — the shared core for every provider that
//! speaks the `/v1/chat/completions` contract. The local-first servers in this
//! ecosystem all expose it: **LM Studio** (`http://localhost:1234/v1`) and a
//! self-hosted **HuggingFace TGI** server both implement OpenAI's chat API, so
//! one request/response shape — parameterized by base endpoint + optional bearer
//! key — drives them with the full agentic tool-calling loop (`converse`) and
//! structured-output (`response_format`). No code leaves the machine for the
//! local endpoints; the provider factory only requires egress opt-in for genuine
//! cloud hosts.

use crate::error::{AutoAgentError, Result};
use crate::planning::llm::provider::{LlmProvider, Message, PlanRequest, ToolCall, ToolSpec, Turn};

/// An OpenAI-compatible chat provider. `endpoint` is the base (e.g.
/// `http://localhost:1234/v1`); `/chat/completions` is appended. `api_key` is
/// sent as a `Bearer` header when present (LM Studio needs none; a TGI server
/// behind auth or the OpenAI cloud does).
pub struct OpenAiCompat {
    name: &'static str,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl OpenAiCompat {
    pub fn new(
        name: &'static str,
        endpoint: String,
        model: String,
        api_key: Option<String>,
    ) -> Self {
        Self {
            name,
            endpoint,
            model,
            api_key,
            client: reqwest::Client::new(),
        }
    }

    fn url(&self) -> String {
        format!("{}/chat/completions", self.endpoint.trim_end_matches('/'))
    }

    /// POST a body to the chat endpoint and parse the JSON response. `what`
    /// labels the call in error messages.
    async fn send(&self, body: &serde_json::Value, what: &str) -> Result<serde_json::Value> {
        let mut rb = self.client.post(self.url()).json(body);
        if let Some(key) = &self.api_key {
            rb = rb.header("authorization", format!("Bearer {key}"));
        }
        let resp = rb.send().await.map_err(|e| {
            AutoAgentError::Llm(format!("{} {what} request failed: {e}", self.name))
        })?;
        resp.json().await.map_err(|e| {
            AutoAgentError::Llm(format!("{} {what} response not JSON: {e}", self.name))
        })
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompat {
    fn name(&self) -> &str {
        self.name
    }

    async fn complete(&self, req: &PlanRequest) -> Result<String> {
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "user", "content": format!("{}\n\n{}", req.objective, req.context)}
            ],
            "stream": false,
        });
        // Structured outputs: force a valid JSON object. `json_object` is the
        // broadly-supported mode (OpenAI, LM Studio, TGI); the prompt already
        // carries the concrete schema, so the model emits a conforming plan.
        if req.format.is_some() {
            body["response_format"] = serde_json::json!({"type": "json_object"});
        }
        let value = self.send(&body, "completion").await?;
        message_content(&value).map(str::to_string).ok_or_else(|| {
            AutoAgentError::Llm(format!("{} response missing message content", self.name))
        })
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn converse(&self, msgs: &[Message], tools: &[ToolSpec]) -> Result<Turn> {
        let messages: Vec<serde_json::Value> = msgs
            .iter()
            .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
            .collect();
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
        });
        if !tools.is_empty() {
            let specs: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::json!(specs);
        }
        let value = self.send(&body, "chat").await?;
        let message = value
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|m| m.get("message"))
            .ok_or_else(|| {
                AutoAgentError::Llm(format!("{} chat response missing message", self.name))
            })?;

        if let Some(tcs) = message.get("tool_calls").and_then(|t| t.as_array()) {
            let calls: Vec<ToolCall> = tcs
                .iter()
                .enumerate()
                .filter_map(|(i, tc)| {
                    let f = tc.get("function")?;
                    let name = f.get("name")?.as_str()?.to_string();
                    // OpenAI returns `arguments` as a JSON *string*; parse it.
                    // (Some servers send an object — accept both.)
                    let arguments = match f.get("arguments") {
                        Some(serde_json::Value::String(s)) => {
                            serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({}))
                        }
                        Some(v) => v.clone(),
                        None => serde_json::json!({}),
                    };
                    let id = tc
                        .get("id")
                        .and_then(|x| x.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| format!("call_{i}"));
                    Some(ToolCall {
                        id,
                        name,
                        arguments,
                    })
                })
                .collect();
            if !calls.is_empty() {
                return Ok(Turn::ToolCalls(calls));
            }
        }

        let content = message
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        Ok(Turn::Final(content))
    }
}

/// `choices[0].message.content` from an OpenAI chat-completions response.
fn message_content(value: &serde_json::Value) -> Option<&str> {
    value
        .get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?
        .as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// Serve a single fixed chat-completions body; return (base_url, request).
    fn serve(body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.flush().unwrap();
            req
        });
        (format!("http://{addr}"), handle)
    }

    #[tokio::test]
    async fn complete_sends_bearer_and_parses_content() {
        let (endpoint, handle) =
            serve(r#"{"choices":[{"message":{"role":"assistant","content":"THE PLAN"}}]}"#);
        let p = OpenAiCompat::new("lmstudio", endpoint, "m".into(), Some("sk-local".into()));
        let out = p
            .complete(&PlanRequest {
                objective: "o".into(),
                context: "c".into(),
                format: Some(serde_json::json!({"type": "object"})),
            })
            .await
            .unwrap();
        let req = handle.join().unwrap();
        assert_eq!(out, "THE PLAN");
        assert!(req
            .to_lowercase()
            .contains("authorization: bearer sk-local"));
        // Hits the OpenAI-compatible path and requests a JSON object.
        assert!(req.contains("/chat/completions"));
        assert!(req.contains("\"response_format\"") && req.contains("json_object"));
    }

    #[tokio::test]
    async fn no_api_key_means_no_authorization_header() {
        // LM Studio needs no key — none is sent.
        let (endpoint, handle) = serve(r#"{"choices":[{"message":{"content":"ok"}}]}"#);
        let p = OpenAiCompat::new("lmstudio", endpoint, "m".into(), None);
        p.complete(&PlanRequest {
            objective: "o".into(),
            context: "c".into(),
            format: None,
        })
        .await
        .unwrap();
        let req = handle.join().unwrap();
        assert!(!req.to_lowercase().contains("authorization:"));
    }

    #[tokio::test]
    async fn converse_parses_tool_calls_with_string_arguments() {
        // OpenAI-style: `arguments` is a JSON *string*, and there is a tools field.
        let (endpoint, handle) = serve(
            r#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[
                {"id":"call_abc","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}}]}}]}"#,
        );
        let p = OpenAiCompat::new("lmstudio", endpoint, "m".into(), None);
        let tools = vec![ToolSpec {
            name: "read_file".into(),
            description: "read".into(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let turn = p
            .converse(
                &[Message {
                    role: "user".into(),
                    content: "go".into(),
                }],
                &tools,
            )
            .await
            .unwrap();
        let req = handle.join().unwrap();
        assert!(req.contains("\"tools\"") && req.contains("read_file"));
        match turn {
            Turn::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].id, "call_abc");
                assert_eq!(calls[0].name, "read_file");
                assert_eq!(calls[0].arguments["path"], "src/lib.rs");
            }
            Turn::Final(_) => panic!("expected tool calls"),
        }
    }

    #[tokio::test]
    async fn converse_returns_final_when_no_tool_calls() {
        let (endpoint, handle) =
            serve(r#"{"choices":[{"message":{"role":"assistant","content":"DONE"}}]}"#);
        let p = OpenAiCompat::new("hf-tgi", endpoint, "m".into(), None);
        let turn = p
            .converse(
                &[Message {
                    role: "user".into(),
                    content: "go".into(),
                }],
                &[],
            )
            .await
            .unwrap();
        handle.join().unwrap();
        match turn {
            Turn::Final(t) => assert_eq!(t, "DONE"),
            Turn::ToolCalls(_) => panic!("expected a final answer"),
        }
    }

    #[tokio::test]
    async fn complete_errors_on_missing_content() {
        let (endpoint, handle) = serve(r#"{"choices":[]}"#);
        let err = OpenAiCompat::new("lmstudio", endpoint, "m".into(), None)
            .complete(&PlanRequest {
                objective: "o".into(),
                context: "c".into(),
                format: None,
            })
            .await
            .unwrap_err();
        handle.join().unwrap();
        assert_eq!(err.error_code(), "llm");
        assert!(
            err.to_string().contains("missing message content"),
            "got: {err}"
        );
    }
}
