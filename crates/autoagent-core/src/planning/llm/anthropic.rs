//! Anthropic cloud provider (M3). API key comes from the environment, never
//! from `Autoagent.toml` (SPEC-1 §3.7 secrets rule). Cloud egress is opt-in and
//! enforced by the provider factory (see `config`).

use crate::error::{AutoAgentError, Result};
use crate::planning::llm::provider::{LlmProvider, PlanRequest};

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    model: String,
    api_key: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(model: String, api_key: String) -> Self {
        Self {
            model,
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

/// Build (but do not send) the Anthropic request — used by tests to assert the
/// header/body contract without a live call.
pub fn build_anthropic_request(model: &str, api_key: &str, prompt: &str) -> reqwest::Request {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "messages": [{"role": "user", "content": prompt}],
    });
    reqwest::Client::new()
        .post(ENDPOINT)
        .header("x-api-key", api_key)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .build()
        .expect("valid anthropic request")
}

#[async_trait::async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn complete(&self, req: &PlanRequest) -> Result<String> {
        let prompt = format!("{}\n\n{}", req.objective, req.context);
        let request = build_anthropic_request(&self.model, &self.api_key, &prompt);
        let resp = self
            .client
            .execute(request)
            .await
            .map_err(|e| AutoAgentError::Llm(format!("anthropic request failed: {e}")))?;
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AutoAgentError::Llm(format!("anthropic response not JSON: {e}")))?;
        // messages API: { "content": [ { "type": "text", "text": "..." } ] }
        value
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|m| m.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AutoAgentError::Llm("anthropic response missing content text".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_request_has_api_version_and_key_header() {
        let req = build_anthropic_request("claude-opus-4-8", "sk-test", "hi");
        assert_eq!(
            req.headers().get("anthropic-version").unwrap(),
            "2023-06-01"
        );
        assert!(req.headers().contains_key("x-api-key"));
        assert_eq!(req.url().as_str(), ENDPOINT);
    }
}
