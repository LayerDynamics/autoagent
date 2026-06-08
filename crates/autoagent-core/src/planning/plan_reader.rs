//! Read a structured JSON plan from disk (SPEC-1 §3.4.1).

use crate::error::{AutoAgentError, Result};
use crate::planning::plan::Plan;
use camino::Utf8Path;

pub fn read_plan(path: &Utf8Path) -> Result<Plan> {
    let text = std::fs::read_to_string(path.as_std_path())
        .map_err(|e| AutoAgentError::Plan(format!("cannot read {path}: {e}")))?;
    serde_json::from_str(&text).map_err(|e| AutoAgentError::Plan(e.to_string()))
}
