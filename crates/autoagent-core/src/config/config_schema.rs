//! Typed `Autoagent.toml` schema (SPEC-1 §6 / Appendix C).

use crate::error::{AutoAgentError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoAgentConfig {
    pub project: ProjectConfig,
    pub agent: AgentConfig,
    pub workspace: WorkspaceConfig,
    pub commands: CommandsConfig,
    pub safety: SafetyConfig,
    pub memory: MemoryConfig,
    pub logging: LoggingConfig,
    pub patches: PatchesConfig,
    pub runs: RunsConfig,
    /// Optional LLM provider config (M3+). Absent in M1/M2 configs.
    #[serde(default)]
    pub llm: Option<LlmConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Local (no egress): "local" (Ollama), "lmstudio", "huggingface-local"
    /// (a self-hosted OpenAI-compatible TGI server). Cloud (egress opt-in +
    /// env key): "anthropic", "openai", "huggingface" (hosted Inference API).
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Source code is only sent to a cloud provider when this is true.
    #[serde(default)]
    pub code_egress_opt_in: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub project_type: String,
    pub language: String,
    pub package_manager: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub mode: String,
    pub allow_self_modification: bool,
    pub max_steps_per_run: u32,
    pub require_approval_before_write: bool,
    pub require_approval_before_command: bool,
    /// Bounded autonomous execution (opt-in). When true, a `run` keeps performing
    /// the NEXT step toward the SAME objective across validated cycles until the
    /// model reports completion or `max_steps_per_run` is exhausted. It never
    /// invents new objectives and never bypasses the policy/snapshot/approval
    /// gates. Default `false`. Absent in older configs (serde default).
    #[serde(default)]
    pub autonomous: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub root: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandsConfig {
    pub test: String,
    pub lint: String,
    pub format: String,
    pub build: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    pub allowed_write_paths: Vec<String>,
    pub blocked_write_paths: Vec<String>,
    pub allowed_commands: Vec<String>,
    pub blocked_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub enabled: bool,
    pub directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub directory: String,
    pub level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchesConfig {
    pub directory: String,
    pub create_before_write: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunsConfig {
    pub directory: String,
}

impl AutoAgentConfig {
    /// Parse config from a TOML string. Any parse failure is a `Config` error
    /// (SPEC-1 FR-4: refuse rather than guess defaults).
    pub fn from_toml_str(s: &str) -> Result<Self> {
        toml::from_str(s).map_err(|e| AutoAgentError::Config(e.to_string()))
    }

    /// Load `Autoagent.toml` from a workspace root.
    pub fn load(root: &camino::Utf8Path) -> Result<Self> {
        let path = root.join("Autoagent.toml");
        let text = std::fs::read_to_string(path.as_std_path())
            .map_err(|_| AutoAgentError::Config(format!("no Autoagent.toml at {path}")))?;
        Self::from_toml_str(&text)
    }
}
