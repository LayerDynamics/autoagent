//! Revert — restore a run's affected files from its `before/` snapshots
//! (SPEC-1 FR-12, §3.5 revert flow). Drift is detected by comparing each file's
//! current sha256 against the recorded `after_hash`; a mismatch is surfaced and
//! that file is left untouched rather than blindly overwritten.

use crate::config::config_schema::AutoAgentConfig;
use crate::editing::snapshot_manager::sha256_hex;
use crate::error::{AutoAgentError, Result};
use crate::logging::event_log::{event_types, EventLog};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;

pub fn revert(root: &Utf8Path, run_id: &str) -> Result<()> {
    let real_root = canonical(root);
    let config = AutoAgentConfig::load(root)?;
    let runs_dir = real_root.join(&config.runs.directory).join(run_id);
    let run_json_path = runs_dir.join("run.json");

    let text = std::fs::read_to_string(run_json_path.as_std_path())
        .map_err(|e| AutoAgentError::Revert(format!("cannot read run {run_id}: {e}")))?;
    let mut run: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| AutoAgentError::Revert(e.to_string()))?;

    let files = run
        .get("files_modified")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut events = EventLog::new(runs_dir.join("events.jsonl"), run_id.to_string());
    events.emit(
        event_types::REVERT_STARTED,
        "Reverted",
        json!({"files": files.len()}),
    )?;

    let mut restored = 0usize;
    let mut drift = 0usize;

    // Reverse order so a rename's created destination is removed before its
    // source snapshot is restored.
    for entry in files.iter().rev() {
        let path = entry
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        let before_hash = entry
            .get("before_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let after_hash = entry
            .get("after_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let abs = real_root.join(path);

        // Drift check: if a regular file is present, it must match what we wrote.
        if abs.as_std_path().is_file() {
            let current = sha256_hex(&std::fs::read(abs.as_std_path())?);
            if !after_hash.is_empty() && current != after_hash {
                events.emit(
                    event_types::DRIFT_DETECTED,
                    "Reverted",
                    json!({"path": path, "expected_hash": after_hash, "actual_hash": current}),
                )?;
                drift += 1;
                continue;
            }
        }

        if before_hash.is_empty() {
            // The run created this path → remove it (file or directory).
            let p = abs.as_std_path();
            if p.is_dir() {
                std::fs::remove_dir_all(p)?;
                restored += 1;
            } else if p.exists() {
                std::fs::remove_file(p)?;
                restored += 1;
            }
        } else {
            // The run modified or deleted this file → restore the snapshot.
            let snap = runs_dir.join("before").join(path);
            if !snap.as_std_path().exists() {
                return Err(AutoAgentError::Revert(format!(
                    "missing before/ snapshot for {path}"
                )));
            }
            if let Some(parent) = abs.parent() {
                std::fs::create_dir_all(parent.as_std_path())?;
            }
            std::fs::copy(snap.as_std_path(), abs.as_std_path())?;
            restored += 1;
        }
    }

    run["state"] = json!("Reverted");
    run["reverted_at"] = json!(chrono::Utc::now().to_rfc3339());
    let out =
        serde_json::to_string_pretty(&run).map_err(|e| AutoAgentError::Serde(e.to_string()))?;
    std::fs::write(run_json_path.as_std_path(), out)?;

    events.emit(
        event_types::REVERT_COMPLETED,
        "Reverted",
        json!({"restored_files": restored, "drift_detected": drift}),
    )?;
    Ok(())
}

fn canonical(root: &Utf8Path) -> Utf8PathBuf {
    std::fs::canonicalize(root.as_std_path())
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .unwrap_or_else(|| root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revert_restores_modified_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("Autoagent.toml"),
            crate::config::default_config::default_toml(),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("crates")).unwrap();
        std::fs::write(root.join("crates/a.rs"), "ORIGINAL").unwrap();
        let plan = root.join("p.json");
        std::fs::write(
            &plan,
            r#"{"objective":"edit","summary":"s","files_to_read":[],
          "files_to_create":[],"files_to_modify":[{"path":"crates/a.rs","purpose":"x"}],
          "operations":[{"kind":"Replace","path":"crates/a.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"CHANGED"}],
          "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
        )
        .unwrap();
        let run_id = crate::runtime::agent_loop::apply(root, &plan, true).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("crates/a.rs")).unwrap(),
            "CHANGED"
        );
        revert(root, &run_id).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("crates/a.rs")).unwrap(),
            "ORIGINAL"
        );
    }

    #[test]
    fn revert_deletes_created_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("Autoagent.toml"),
            crate::config::default_config::default_toml(),
        )
        .unwrap();
        let plan = root.join("p.json");
        std::fs::write(
            &plan,
            r#"{"objective":"new","summary":"s","files_to_read":[],
          "files_to_create":[{"path":"crates/new.rs","purpose":"x"}],"files_to_modify":[],
          "operations":[{"kind":"Create","path":"crates/new.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"X"}],
          "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
        )
        .unwrap();
        let run_id = crate::runtime::agent_loop::apply(root, &plan, true).unwrap();
        assert!(root.join("crates/new.rs").as_std_path().exists());
        revert(root, &run_id).unwrap();
        assert!(!root.join("crates/new.rs").as_std_path().exists());
    }
}
