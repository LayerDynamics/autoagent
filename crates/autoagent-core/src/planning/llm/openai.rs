//! OpenAI cloud provider (M3). API key from the environment; cloud egress is
//! opt-in and enforced by the provider factory (see `config`).

use crate::error::{AutoAgentError, Result};
use crate::planning::llm::provider::{LlmProvider, PlanRequest};

const ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";

pub struct OpenAiProvider {
    model: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(model: String, api_key: String) -> Self {
        Self {
            model,
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

/// Build (but do not send) the OpenAI request — used by tests to assert the
/// header/body contract without a live call.
pub fn build_openai_request(model: &str, api_key: &str, prompt: &str) -> reqwest::Request {
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
    });
    reqwest::Client::new()
        .post(ENDPOINT)
        .header("authorization", format!("Bearer {api_key}"))
        .header("content-type", "application/json")
        .json(&body)
        .build()
        .expect("valid openai request")
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn complete(&self, req: &PlanRequest) -> Result<String> {
        let prompt = format!("{}\n\n{}", req.objective, req.context);
        let request = build_openai_request(&self.model, &self.api_key, &prompt);
        let resp = self
            .client
            .execute(request)
            .await
            .map_err(|e| AutoAgentError::Llm(format!("openai request failed: {e}")))?;
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AutoAgentError::Llm(format!("openai response not JSON: {e}")))?;
        // chat API: { "choices": [ { "message": { "content": "..." } } ] }
        value
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|m| m.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AutoAgentError::Llm("openai response missing message content".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_request_has_bearer_auth() {
        let req = build_openai_request("gpt-4", "sk-test", "hi");
        let auth = req
            .headers()
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(auth.starts_with("Bearer "));
        assert_eq!(req.url().as_str(), ENDPOINT);
    }
}
