//! Per-run identity and policy snapshot (SPEC-1 §3.3 / §8.3).

use crate::config::config_schema::AutoAgentConfig;
use crate::runtime::run_state::RunState;
use camino::Utf8PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub id: Uuid,
    pub run_id: String,
    pub objective: String,
    pub root_directory: Utf8PathBuf,
    pub mode: AgentMode,
    pub self_modification: bool,
    pub state: RunState,
    pub config: AutoAgentConfig,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentMode {
    PlanOnly,
    Supervised,
    Apply,
    Autonomous,
    Evolve,
}
