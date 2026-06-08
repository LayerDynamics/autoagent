//! Run logger — owns the run-folder layout and serializes `run.json`
//! (SPEC-1 §3.4.2 / §9.1). Accumulates per-run facts then writes them once.

use crate::error::Result;
use camino::Utf8PathBuf;
use chrono::Utc;
use uuid::Uuid;

pub struct RunLogger {
    run_dir: Utf8PathBuf,
    run_id: String,
    task_id: Uuid,
    objective: String,
    mode: String,
    self_modification: bool,
    state: String,
    started_at: chrono::DateTime<Utc>,
    plan_path: Option<String>,
    patch_path: Option<String>,
    files_read: Vec<String>,
    files_modified: Vec<serde_json::Value>,
    commands_executed: Vec<serde_json::Value>,
    approvals: Vec<serde_json::Value>,
    reverted_at: Option<String>,
}

impl RunLogger {
    /// Create the run folder (with `before/` and `after/`) and write `objective.md`.
    pub fn create(run_dir: Utf8PathBuf, run_id: String, objective: String) -> Result<Self> {
        std::fs::create_dir_all(run_dir.join("before").as_std_path())?;
        std::fs::create_dir_all(run_dir.join("after").as_std_path())?;
        std::fs::write(
            run_dir.join("objective.md").as_std_path(),
            format!("# Objective\n\n{objective}\n"),
        )?;
        Ok(Self {
            run_dir,
            run_id,
            task_id: Uuid::new_v4(),
            objective,
            mode: "Apply".into(),
            self_modification: false,
            state: "Created".into(),
            started_at: Utc::now(),
            plan_path: None,
            patch_path: None,
            files_read: Vec::new(),
            files_modified: Vec::new(),
            commands_executed: Vec::new(),
            approvals: Vec::new(),
            reverted_at: None,
        })
    }

    pub fn run_dir(&self) -> &Utf8PathBuf {
        &self.run_dir
    }
    pub fn task_id(&self) -> Uuid {
        self.task_id
    }
    pub fn set_mode(&mut self, mode: &str, self_modification: bool) {
        self.mode = mode.to_string();
        self.self_modification = self_modification;
    }
    pub fn set_state(&mut self, state: &str) {
        self.state = state.to_string();
    }
    pub fn set_plan_path(&mut self, p: &str) {
        self.plan_path = Some(p.to_string());
    }
    pub fn set_patch_path(&mut self, p: &str) {
        self.patch_path = Some(p.to_string());
    }
    pub fn set_files_read(&mut self, files: Vec<String>) {
        self.files_read = files;
    }
    pub fn record_file(&mut self, path: &str, kind: &str, before_hash: &str, after_hash: &str) {
        self.record_file_full(path, kind, None, before_hash, after_hash);
    }

    /// Record a file change, including a rename destination when applicable.
    /// `before_hash` empty means the file did not exist before (a create);
    /// `after_hash` empty means it no longer exists after (a delete/rename source).
    pub fn record_file_full(
        &mut self,
        path: &str,
        kind: &str,
        destination: Option<&str>,
        before_hash: &str,
        after_hash: &str,
    ) {
        self.files_modified.push(serde_json::json!({
            "path": path, "kind": kind, "destination_path": destination,
            "before_hash": before_hash, "after_hash": after_hash,
        }));
    }
    pub fn record_command(&mut self, command: &str, exit_code: Option<i32>, duration_ms: u128) {
        self.commands_executed.push(serde_json::json!({
            "command": command, "exit_code": exit_code, "duration_ms": duration_ms,
        }));
    }
    pub fn record_approval(&mut self, kind: &str, granted: bool) {
        self.approvals.push(serde_json::json!({
            "kind": kind, "granted": granted, "at": Utc::now().to_rfc3339(),
        }));
    }
    pub fn mark_reverted(&mut self) {
        self.state = "Reverted".into();
        self.reverted_at = Some(Utc::now().to_rfc3339());
    }

    /// Serialize and write `run.json`.
    pub fn finish(&mut self, validation_passed: bool) -> Result<()> {
        let ended = Utc::now();
        let duration_ms = (ended - self.started_at).num_milliseconds().max(0);
        let value = serde_json::json!({
            "run_id": self.run_id,
            "task_id": self.task_id.to_string(),
            "objective": self.objective,
            "mode": self.mode,
            "self_modification": self.self_modification,
            "state": self.state,
            "started_at": self.started_at.to_rfc3339(),
            "ended_at": ended.to_rfc3339(),
            "duration_ms": duration_ms,
            "plan_path": self.plan_path,
            "files_read": self.files_read,
            "files_modified": self.files_modified,
            "commands_executed": self.commands_executed,
            "validation_passed": validation_passed,
            "patch_path": self.patch_path,
            "approvals": self.approvals,
            "reverted_at": self.reverted_at,
        });
        let text = serde_json::to_string_pretty(&value)
            .map_err(|e| crate::error::AutoAgentError::Serde(e.to_string()))?;
        std::fs::write(self.run_dir.join("run.json").as_std_path(), text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_run_json_with_required_fields() {
        let dir = tempfile::tempdir().unwrap();
        let run_dir = camino::Utf8Path::from_path(dir.path())
            .unwrap()
            .to_path_buf();
        let mut rl =
            RunLogger::create(run_dir.clone(), "20260608T000000Z-x".into(), "obj".into()).unwrap();
        rl.set_state("Completed");
        rl.record_file("a.txt", "Replace", "h1", "h2");
        rl.finish(true).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(run_dir.join("run.json").as_std_path()).unwrap(),
        )
        .unwrap();
        assert_eq!(v["state"], "Completed");
        assert_eq!(v["validation_passed"], true);
        assert_eq!(v["files_modified"][0]["path"], "a.txt");
        assert_eq!(v["run_id"], "20260608T000000Z-x");
        assert!(run_dir.join("objective.md").as_std_path().exists());
    }
}
