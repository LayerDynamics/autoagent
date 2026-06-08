//! Local LLM provider (M3) — the default provider. Posts to an Ollama-style
//! `/api/generate` endpoint so all source stays on-machine (SPEC-1 FR-22
//! local-model option). No code ever leaves the host with this provider.

use crate::error::{AutoAgentError, Result};
use crate::planning::llm::provider::{LlmProvider, PlanRequest};

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
        let body = serde_json::json!({
            "model": self.model,
            "prompt": format!("{}\n\n{}", req.objective, req.context),
            "stream": false,
        });
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
            })
            .await
            .unwrap();
        assert_eq!(out, "GENERATED");
        handle.join().unwrap();
    }
}
