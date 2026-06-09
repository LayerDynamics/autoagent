//! Append-only `events.jsonl` writer with a monotonic per-run sequence
//! (SPEC-1 §3.4.3). The event catalog lives in `event_types` to prevent typos.

use crate::error::Result;
use camino::Utf8PathBuf;
use std::io::Write;

/// Canonical event `type` strings (SPEC-1 §3.4.3 catalog).
pub mod event_types {
    pub const RUN_STARTED: &str = "run_started";
    pub const CONFIG_LOADED: &str = "config_loaded";
    pub const ANALYSIS_COMPLETED: &str = "analysis_completed";
    pub const MEMORY_LOADED: &str = "memory_loaded";
    pub const PLAN_LOADED: &str = "plan_loaded";
    pub const PLAN_REJECTED: &str = "plan_rejected";
    pub const APPROVAL_REQUESTED: &str = "approval_requested";
    pub const APPROVAL_GRANTED: &str = "approval_granted";
    pub const APPROVAL_DENIED: &str = "approval_denied";
    pub const SNAPSHOT_CREATED: &str = "snapshot_created";
    pub const OPERATION_APPLIED: &str = "operation_applied";
    pub const OPERATION_FAILED: &str = "operation_failed";
    pub const PATCH_WRITTEN: &str = "patch_written";
    pub const COMMAND_STARTED: &str = "command_started";
    pub const COMMAND_FINISHED: &str = "command_finished";
    pub const VALIDATION_COMPLETED: &str = "validation_completed";
    pub const RUN_COMPLETED: &str = "run_completed";
    pub const RUN_FAILED: &str = "run_failed";
    pub const REVERT_STARTED: &str = "revert_started";
    pub const REVERT_COMPLETED: &str = "revert_completed";
    pub const DRIFT_DETECTED: &str = "drift_detected";
}

pub struct EventLog {
    path: Utf8PathBuf,
    workspace_path: Option<Utf8PathBuf>,
    run_id: String,
    seq: u64,
}

impl EventLog {
    pub fn new(path: Utf8PathBuf, run_id: String) -> Self {
        Self {
            path,
            workspace_path: None,
            run_id,
            seq: 0,
        }
    }

    /// Also mirror every event to a workspace-level aggregate log
    /// (`.agent/logs/events.jsonl`, SPEC-1 FR-10).
    pub fn with_workspace_log(mut self, workspace_path: Utf8PathBuf) -> Self {
        self.workspace_path = Some(workspace_path);
        self
    }

    /// Append one event with the common envelope (SPEC-1 §3.4.3).
    pub fn emit(&mut self, ty: &str, state: &str, data: serde_json::Value) -> Result<()> {
        self.seq += 1;
        if let Some(p) = self.path.parent() {
            std::fs::create_dir_all(p.as_std_path())?;
        }
        let evt = serde_json::json!({
            "schema_version": crate::schema_version::SCHEMA_VERSION,
            "ts": chrono::Utc::now().to_rfc3339(),
            "run_id": self.run_id,
            "seq": self.seq,
            "type": ty,
            "state": state,
            "data": data,
        });
        let line = format!("{evt}");
        append_line(&self.path, &line)?;
        if let Some(ws) = &self.workspace_path {
            append_line(ws, &line)?;
        }
        Ok(())
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }
}

fn append_line(path: &Utf8PathBuf, line: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent.as_std_path())?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.as_std_path())?;
    writeln!(f, "{line}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_monotonic_seq() {
        let dir = tempfile::tempdir().unwrap();
        let path = camino::Utf8Path::from_path(dir.path())
            .unwrap()
            .join("events.jsonl");
        let mut log = EventLog::new(path.clone(), "run-1".into());
        log.emit(
            event_types::RUN_STARTED,
            "Created",
            serde_json::json!({"objective":"o"}),
        )
        .unwrap();
        log.emit(
            event_types::RUN_COMPLETED,
            "Completed",
            serde_json::json!({"state":"Completed"}),
        )
        .unwrap();
        let body = std::fs::read_to_string(path.as_std_path()).unwrap();
        let lines: Vec<_> = body.lines().collect();
        assert_eq!(lines.len(), 2);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["seq"], 1);
        assert_eq!(first["type"], "run_started");
        let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second["seq"], 2);
    }
}
