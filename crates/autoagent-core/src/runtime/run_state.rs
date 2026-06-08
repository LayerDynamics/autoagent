//! Run state machine (SPEC-1 §3.3 / §8.1).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunState {
    Created,
    LoadingConfig,
    AnalyzingProject,
    LoadingMemory,
    Planning,
    AwaitingApproval,
    Snapshotting,
    ApplyingChanges,
    Validating,
    Repairing,
    Completed,
    Failed,
    Reverted,
}

impl RunState {
    /// Stable string used in run.json / events (matches the variant name).
    pub fn as_str(&self) -> &'static str {
        match self {
            RunState::Created => "Created",
            RunState::LoadingConfig => "LoadingConfig",
            RunState::AnalyzingProject => "AnalyzingProject",
            RunState::LoadingMemory => "LoadingMemory",
            RunState::Planning => "Planning",
            RunState::AwaitingApproval => "AwaitingApproval",
            RunState::Snapshotting => "Snapshotting",
            RunState::ApplyingChanges => "ApplyingChanges",
            RunState::Validating => "Validating",
            RunState::Repairing => "Repairing",
            RunState::Completed => "Completed",
            RunState::Failed => "Failed",
            RunState::Reverted => "Reverted",
        }
    }
}
