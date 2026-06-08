//! Provider factory (M3) — builds an `LlmProvider` from `[llm]` config and
//! enforces the SPEC-1 FR-22 invariants: cloud providers require explicit
//! `code_egress_opt_in` AND an API key from the environment (never config).

use crate::config::config_schema::LlmConfig;
use crate::error::{AutoAgentError, Result};
use crate::planning::llm::anthropic::AnthropicProvider;
use crate::planning::llm::local::LocalProvider;
use crate::planning::llm::openai::OpenAiProvider;
use crate::planning::llm::provider::LlmProvider;

const DEFAULT_LOCAL_ENDPOINT: &str = "http://localhost:11434";

/// Build a provider from config. The local provider is the safe default and
/// never requires opt-in (no egress). Cloud providers are gated.
pub fn build_provider(cfg: &LlmConfig) -> Result<Box<dyn LlmProvider>> {
    match cfg.provider.as_str() {
        "local" => {
            let endpoint = cfg
                .endpoint
                .clone()
                .unwrap_or_else(|| DEFAULT_LOCAL_ENDPOINT.to_string());
            Ok(Box::new(LocalProvider::new(endpoint, cfg.model.clone())))
        }
        "anthropic" => {
            require_egress(cfg)?;
            let key = env_key("ANTHROPIC_API_KEY")?;
            Ok(Box::new(AnthropicProvider::new(cfg.model.clone(), key)))
        }
        "openai" => {
            require_egress(cfg)?;
            let key = env_key("OPENAI_API_KEY")?;
            Ok(Box::new(OpenAiProvider::new(cfg.model.clone(), key)))
        }
        other => Err(AutoAgentError::Llm(format!(
            "unknown llm provider '{other}'"
        ))),
    }
}

fn require_egress(cfg: &LlmConfig) -> Result<()> {
    if cfg.code_egress_opt_in {
        Ok(())
    } else {
        Err(AutoAgentError::Llm(
            "cloud provider requires code_egress_opt_in = true (source code would leave the machine)"
                .into(),
        ))
    }
}

fn env_key(var: &str) -> Result<String> {
    std::env::var(var).map_err(|_| AutoAgentError::Llm(format!("missing {var} in environment")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(provider: &str, opt_in: bool) -> LlmConfig {
        LlmConfig {
            provider: provider.into(),
            model: "m".into(),
            endpoint: None,
            code_egress_opt_in: opt_in,
        }
    }

    #[test]
    fn local_provider_builds_without_opt_in() {
        assert!(build_provider(&cfg("local", false)).is_ok());
    }

    #[test]
    fn cloud_without_opt_in_is_refused() {
        match build_provider(&cfg("anthropic", false)) {
            Err(e) => {
                assert_eq!(e.error_code(), "llm");
                assert!(e.to_string().contains("code_egress_opt_in"));
            }
            Ok(_) => panic!("cloud provider without opt-in must be refused"),
        }
    }

    #[test]
    fn unknown_provider_is_refused() {
        assert!(build_provider(&cfg("gemini", true)).is_err());
    }
}
