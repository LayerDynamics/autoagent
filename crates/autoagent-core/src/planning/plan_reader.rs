//! Read a structured JSON plan from disk (SPEC-1 §3.4.1).

use crate::error::{AutoAgentError, Result};
use crate::planning::plan::Plan;
use camino::Utf8Path;

pub fn read_plan(path: &Utf8Path) -> Result<Plan> {
    let text = std::fs::read_to_string(path.as_std_path())
        .map_err(|e| AutoAgentError::Plan(format!("cannot read {path}: {e}")))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| AutoAgentError::Plan(e.to_string()))?;
    // Reject a plan that declares a newer schema than this build supports.
    if let Some(v) = value.get("schema_version").and_then(|x| x.as_u64()) {
        crate::schema_version::accepts_version(v as u32)?;
    }
    serde_json::from_value(value).map_err(|e| AutoAgentError::Plan(e.to_string()))
}

/// True if a plan reader accepts the given schema version (for tests/tools).
pub fn accepts_version(v: u32) -> Result<()> {
    crate::schema_version::accepts_version(v)
}
