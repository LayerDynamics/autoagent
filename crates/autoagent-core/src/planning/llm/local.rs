//! Local LLM provider (M3) — the default provider. Posts to an Ollama-style
//! `/api/generate` endpoint so all source stays on-machine (SPEC-1 FR-22
//! local-model option). No code ever leaves the host with this provider.

use crate::error::{AutoAgentError, Result};
use crate::planning::llm::provider::{LlmProvider, Message, PlanRequest, ToolCall, ToolSpec, Turn};

pub struct LocalProvider {
    endpoint: String,
    model: String,
    client: reqwest::Client,
}

impl LocalProvider {
    pub fn new(endpoint: String, model: String) -> Self {
        Self {
            endpoint,
            model,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for LocalProvider {
    fn name(&self) -> &str {
        "local"
    }

    async fn complete(&self, req: &PlanRequest) -> Result<String> {
        let url = format!("{}/api/generate", self.endpoint.trim_end_matches('/'));
        let mut body = serde_json::json!({
            "model": self.model,
            "prompt": format!("{}\n\n{}", req.objective, req.context),
            "stream": false,
        });
        // Ollama structured outputs: a JSON-Schema `format` forces the model to
        // emit a conforming object, so the planner never sees malformed JSON.
        if let Some(fmt) = &req.format {
            body["format"] = fmt.clone();
        }
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AutoAgentError::Llm(format!("local request failed: {e}")))?;
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AutoAgentError::Llm(format!("local response not JSON: {e}")))?;
        value
            .get("response")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AutoAgentError::Llm("local response missing 'response' field".into()))
    }

    fn supports_tools(&self) -> bool {
        true
    }

    /// Drive one turn of the agentic loop via Ollama's `/api/chat` tool-calling.
    /// Returns `ToolCalls` when the model asks to run tools, else `Final`. If the
    /// model/endpoint does not support tools the call errors and the caller falls
    /// back to the one-shot path.
    async fn converse(&self, msgs: &[Message], tools: &[ToolSpec]) -> Result<Turn> {
        let url = format!("{}/api/chat", self.endpoint.trim_end_matches('/'));
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
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AutoAgentError::Llm(format!("local chat request failed: {e}")))?;
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AutoAgentError::Llm(format!("local chat response not JSON: {e}")))?;
        let message = value
            .get("message")
            .ok_or_else(|| AutoAgentError::Llm("local chat response missing 'message'".into()))?;

        if let Some(tcs) = message.get("tool_calls").and_then(|t| t.as_array()) {
            let calls: Vec<ToolCall> = tcs
                .iter()
                .enumerate()
                .filter_map(|(i, tc)| {
                    let f = tc.get("function")?;
                    let name = f.get("name")?.as_str()?.to_string();
                    let arguments = f.get("arguments").cloned().unwrap_or(serde_json::json!({}));
                    Some(ToolCall {
                        id: format!("call_{i}"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[tokio::test]
    async fn posts_to_configured_endpoint() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let body = r#"{"response":"GENERATED"}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let provider = LocalProvider::new(format!("http://{addr}"), "m".into());
        let out = provider
            .complete(&PlanRequest {
                objective: "o".into(),
                context: "c".into(),
                format: None,
            })
            .await
            .unwrap();
        assert_eq!(out, "GENERATED");
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn forwards_format_schema_to_ollama() {
        // Capture the request body and assert the schema is forwarded as `format`.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let body = r#"{"response":"[]"}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.flush().unwrap();
            req
        });

        let provider = LocalProvider::new(format!("http://{addr}"), "m".into());
        provider
            .complete(&PlanRequest {
                objective: "o".into(),
                context: "c".into(),
                format: Some(serde_json::json!({"type": "array"})),
            })
            .await
            .unwrap();
        let req = handle.join().unwrap();
        assert!(
            req.contains("\"format\"") && req.contains("\"array\""),
            "request body must carry the JSON-schema format: {req}"
        );
    }

    /// Serve a fixed `/api/chat` body and return the captured request.
    fn serve_chat(body: &'static str) -> (String, std::thread::JoinHandle<String>) {
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
    async fn converse_returns_tool_calls_when_model_requests_them() {
        let (endpoint, handle) = serve_chat(
            r#"{"message":{"role":"assistant","content":"","tool_calls":[
                {"function":{"name":"read_file","arguments":{"path":"src/lib.rs"}}}]}}"#,
        );
        let provider = LocalProvider::new(endpoint, "m".into());
        let tools = vec![ToolSpec {
            name: "read_file".into(),
            description: "read".into(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let turn = provider
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
        assert!(req.contains("/api/chat") && req.contains("\"tools\""));
        match turn {
            Turn::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "read_file");
                assert_eq!(calls[0].arguments["path"], "src/lib.rs");
            }
            Turn::Final(_) => panic!("expected tool calls"),
        }
    }

    #[tokio::test]
    async fn converse_returns_final_when_model_answers() {
        let (endpoint, handle) =
            serve_chat(r#"{"message":{"role":"assistant","content":"THE PLAN"}}"#);
        let provider = LocalProvider::new(endpoint, "m".into());
        let turn = provider
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
            Turn::Final(text) => assert_eq!(text, "THE PLAN"),
            Turn::ToolCalls(_) => panic!("expected a final answer"),
        }
    }
}
