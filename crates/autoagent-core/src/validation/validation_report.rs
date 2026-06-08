//! Structured validation command outcomes (SPEC-1 §3.3 / §8.5).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub passed: bool,
    pub commands: Vec<CommandValidationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandValidationResult {
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
}
