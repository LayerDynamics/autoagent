//! HuggingFace Inference API provider (cloud). Posts to the hosted text-
//! generation Inference API — or a dedicated Inference Endpoint URL supplied via
//! `[llm] endpoint` — authenticated with `HF_TOKEN` from the environment (never
//! config, per the secrets rule). This is a CLOUD provider: the factory gates it
//! behind `code_egress_opt_in` because the prompt (which may carry source code)
//! leaves the machine. For a *local* HuggingFace TGI server, use the
//! `huggingface-local` provider (OpenAI-compatible, on-machine, no egress).

use crate::error::{AutoAgentError, Result};
use crate::planning::llm::provider::{LlmProvider, PlanRequest};

pub struct HuggingFaceProvider {
    endpoint: String,
    api_key: String,
    client: reqwest::Client,
}

impl HuggingFaceProvider {
    /// `endpoint` is the full Inference API / Endpoint URL (the factory defaults
    /// it to `https://api-inference.huggingface.co/models/<model>`).
    pub fn new(endpoint: String, api_key: String) -> Self {
        Self {
            endpoint,
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for HuggingFaceProvider {
    fn name(&self) -> &str {
        "huggingface"
    }

    async fn complete(&self, req: &PlanRequest) -> Result<String> {
        let body = serde_json::json!({
            "inputs": format!("{}\n\n{}", req.objective, req.context),
            "parameters": {"return_full_text": false},
        });
        let resp = self
            .client
            .post(&self.endpoint)
            .header("authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| AutoAgentError::Llm(format!("huggingface request failed: {e}")))?;
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AutoAgentError::Llm(format!("huggingface response not JSON: {e}")))?;
        // Surface a structured HF error (`{"error": "..."}`) rather than a vague
        // missing-field message.
        if let Some(err) = value.get("error").and_then(|e| e.as_str()) {
            return Err(AutoAgentError::Llm(format!("huggingface error: {err}")));
        }
        // text-generation returns `[{"generated_text":"..."}]`; some endpoints
        // return the bare object `{"generated_text":"..."}`.
        generated_text(&value).map(str::to_string).ok_or_else(|| {
            AutoAgentError::Llm("huggingface response missing generated_text".into())
        })
    }
}

/// `generated_text` from either the array or bare-object Inference API shape.
fn generated_text(value: &serde_json::Value) -> Option<&str> {
    let obj = match value {
        serde_json::Value::Array(a) => a.first()?,
        other => other,
    };
    obj.get("generated_text")?.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

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

    async fn run(endpoint: String) -> Result<String> {
        HuggingFaceProvider::new(endpoint, "hf_token".into())
            .complete(&PlanRequest {
                objective: "o".into(),
                context: "c".into(),
                format: None,
            })
            .await
    }

    #[tokio::test]
    async fn sends_bearer_and_inputs_then_parses_array_shape() {
        let (endpoint, handle) = serve(r#"[{"generated_text":"THE PLAN"}]"#);
        let out = run(endpoint).await.unwrap();
        let req = handle.join().unwrap();
        assert_eq!(out, "THE PLAN");
        assert!(req
            .to_lowercase()
            .contains("authorization: bearer hf_token"));
        assert!(req.contains("\"inputs\""));
    }

    #[tokio::test]
    async fn parses_bare_object_shape() {
        let (endpoint, handle) = serve(r#"{"generated_text":"BARE"}"#);
        let out = run(endpoint).await.unwrap();
        handle.join().unwrap();
        assert_eq!(out, "BARE");
    }

    #[tokio::test]
    async fn surfaces_hf_error_field() {
        let (endpoint, handle) = serve(r#"{"error":"Model is loading"}"#);
        let err = run(endpoint).await.unwrap_err();
        handle.join().unwrap();
        assert_eq!(err.error_code(), "llm");
        assert!(err.to_string().contains("Model is loading"), "got: {err}");
    }

    #[tokio::test]
    async fn errors_on_missing_generated_text() {
        let (endpoint, handle) = serve(r#"[{"nope":1}]"#);
        let err = run(endpoint).await.unwrap_err();
        handle.join().unwrap();
        assert!(
            err.to_string().contains("missing generated_text"),
            "got: {err}"
        );
    }
}
