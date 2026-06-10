//! The apply loop — executes the SPEC-1 §3.5 apply path for a structured plan:
//! load config → policy engine → read plan → validate → approval → snapshot →
//! apply → record after → patch → run.json. Every applied run is snapshotted
//! and therefore reversible (SPEC-1 FR-7/FR-12).

use crate::config::config_schema::AutoAgentConfig;
use crate::editing::file_editor::FileEditor;
use crate::editing::file_operation::FileOperationKind;
use crate::editing::patch_writer::{self, PatchEntry};
use crate::editing::snapshot_manager::SnapshotManager;
use crate::error::{AutoAgentError, PolicyError, Result};
use crate::logging::event_log::{event_types, EventLog};
use crate::logging::run_logger::RunLogger;
use crate::planning::{plan_reader, plan_validator};
use crate::safety::approval_gate::ApprovalGate;
use crate::safety::policy_engine::PolicyEngine;
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;

/// Apply a plan file. `auto_approve` is the resolved write-approval decision
/// (the CLI translates an interactive prompt into this bool). Returns the run id.
pub fn apply(root: &Utf8Path, plan_path: &Utf8Path, auto_approve: bool) -> Result<String> {
    let config = AutoAgentConfig::load(root)?;
    let real_root = canonical(root);
    let engine = PolicyEngine::from_config(&config, real_root.clone());

    let plan = plan_reader::read_plan(plan_path)?;
    plan_validator::validate_plan(&plan, &engine)?;

    if config.agent.require_approval_before_write && !auto_approve {
        return Err(AutoAgentError::Policy(PolicyError::WriteNotApproved(
            "write approval required (run with --yes or approve interactively)".into(),
        )));
    }

    let run_id = make_run_id(&plan.objective);
    let runs_dir = real_root.join(&config.runs.directory).join(&run_id);
    let mut logger = RunLogger::create(runs_dir.clone(), run_id.clone(), plan.objective.clone())?;
    logger.set_plan_path(plan_path.as_str());
    logger.set_files_read(plan.files_to_read.iter().map(|p| p.to_string()).collect());

    // Run-folder contract (SPEC-1 FR-9): human plan + structured operation list.
    std::fs::write(
        runs_dir.join("plan.md").as_std_path(),
        crate::planning::plan_writer::render_md(&plan),
    )?;
    std::fs::write(
        runs_dir.join("file-operations.json").as_std_path(),
        serde_json::to_string_pretty(&plan.operations)
            .map_err(|e| AutoAgentError::Serde(e.to_string()))?,
    )?;

    // Events mirror to the workspace-level aggregate log (SPEC-1 FR-10).
    let workspace_log = real_root
        .join(&config.logging.directory)
        .join("events.jsonl");
    let mut events = EventLog::new(runs_dir.join("events.jsonl"), run_id.clone())
        .with_workspace_log(workspace_log);
    events.emit(
        event_types::RUN_STARTED,
        "Created",
        json!({"objective": plan.objective}),
    )?;
    events.emit(
        event_types::CONFIG_LOADED,
        "LoadingConfig",
        json!({"config_path": "Autoagent.toml"}),
    )?;
    events.emit(
        event_types::PLAN_LOADED,
        "Planning",
        json!({"plan_path": plan_path.as_str(), "operation_count": plan.operations.len()}),
    )?;

    let snapshots = SnapshotManager::new(runs_dir.clone());
    let editor = FileEditor::new(real_root.clone());
    let mut patch_entries: Vec<PatchEntry> = Vec::new();

    logger.set_state("ApplyingChanges");
    for op in &plan.operations {
        let rel = op.path.clone();
        let abs = real_root.join(&rel);
        let existed = abs.as_std_path().exists();
        let before_content = if existed {
            std::fs::read_to_string(abs.as_std_path()).unwrap_or_default()
        } else {
            String::new()
        };
        let before_hash = if existed {
            let h = snapshots.snapshot(&real_root, rel.clone())?;
            events.emit(
                event_types::SNAPSHOT_CREATED,
                "Snapshotting",
                json!({"path": rel, "before_hash": h}),
            )?;
            h
        } else {
            String::new()
        };

        if let Err(e) = editor.apply(op) {
            events.emit(
                event_types::OPERATION_FAILED,
                "Failed",
                json!({"kind": kind_str(&op.kind), "path": rel, "error_code": e.error_code()}),
            )?;
            logger.set_state("Failed");
            logger.finish(false)?;
            events.emit(
                event_types::RUN_FAILED,
                "Failed",
                json!({"error_code": e.error_code()}),
            )?;
            return Err(e);
        }

        // Hash/snapshot the result only when it is a regular file. Directories
        // (CreateDirectory) and removed paths (Delete / Rename source) have no
        // after-hash.
        let now_is_file = abs.as_std_path().is_file();
        let (after_hash, after_content) = if now_is_file {
            let h = snapshots.record_after(&real_root, rel.clone())?;
            (
                h,
                std::fs::read_to_string(abs.as_std_path()).unwrap_or_default(),
            )
        } else {
            (String::new(), String::new())
        };

        let dest = op.destination_path.as_ref().map(|d| d.as_str());
        logger.record_file_full(
            rel.as_str(),
            kind_str(&op.kind),
            dest,
            &before_hash,
            &after_hash,
        );
        events.emit(
            event_types::OPERATION_APPLIED,
            "ApplyingChanges",
            json!({"kind": kind_str(&op.kind), "path": rel, "after_hash": after_hash}),
        )?;
        patch_entries.push(PatchEntry {
            path: rel.to_string(),
            before: before_content,
            after: after_content,
        });

        // A rename produces a new file at the destination; record it as a
        // create so revert can remove it.
        if let Some(dpath) = &op.destination_path {
            if real_root.join(dpath).as_std_path().exists() {
                let dh = snapshots.record_after(&real_root, dpath.clone())?;
                let dc = std::fs::read_to_string(real_root.join(dpath).as_std_path())
                    .unwrap_or_default();
                logger.record_file_full(dpath.as_str(), "Create", None, "", &dh);
                patch_entries.push(PatchEntry {
                    path: dpath.to_string(),
                    before: String::new(),
                    after: dc,
                });
            }
        }
    }

    let patches_dir = real_root.join(&config.patches.directory);
    let patch_path = patch_writer::write_patch(&patches_dir, &run_id, &patch_entries)?;
    logger.set_patch_path(patch_path.as_str());
    events.emit(
        event_types::PATCH_WRITTEN,
        "ApplyingChanges",
        json!({"patch_path": patch_path}),
    )?;

    // Complete the run-folder contract (SPEC-1 FR-9). `apply` runs no validation
    // commands itself (that is the `run` workflow); the report reflects that and
    // is overwritten by `run` with real results when validation executes.
    std::fs::write(
        runs_dir.join("validation-report.md").as_std_path(),
        "# Validation Report — not run\n\n`apply` does not execute validation commands; use `run` for a validated workflow.\n",
    )?;
    std::fs::write(
        runs_dir.join("summary.md").as_std_path(),
        format!(
            "# Run Summary\n\n- Objective: {}\n- State: Completed\n- Operations applied: {}\n- Patch: {}\n",
            plan.objective,
            plan.operations.len(),
            patch_path
        ),
    )?;

    logger.set_state("Completed");
    logger.finish(true)?;
    events.emit(
        event_types::RUN_COMPLETED,
        "Completed",
        json!({"state": "Completed"}),
    )?;
    Ok(run_id)
}

/// Apply a plan, resolving the write-approval decision through an
/// `ApprovalGate` (the CLI injects an interactive gate; `--yes` injects
/// `AutoGate::allow`). When the config does not require approval, the gate is
/// not consulted.
pub fn apply_with_gate(
    root: &Utf8Path,
    plan_path: &Utf8Path,
    gate: &dyn ApprovalGate,
) -> Result<String> {
    let config = AutoAgentConfig::load(root)?;
    if config.agent.require_approval_before_write {
        gate.confirm_write("planned changes")?;
    }
    apply(root, plan_path, true)
}

fn canonical(root: &Utf8Path) -> Utf8PathBuf {
    std::fs::canonicalize(root.as_std_path())
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .unwrap_or_else(|| root.to_path_buf())
}

/// `<UTC compact>-<slug>` run id (SPEC-1 §3.4.2).
fn make_run_id(objective: &str) -> String {
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    format!("{ts}-{}", slug(objective))
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in s.chars().take(40) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "run".into()
    } else {
        trimmed
    }
}

pub fn kind_str(kind: &FileOperationKind) -> &'static str {
    use FileOperationKind::*;
    match kind {
        Create => "Create",
        Write => "Write",
        Replace => "Replace",
        Append => "Append",
        Delete => "Delete",
        Rename => "Rename",
        CreateDirectory => "CreateDirectory",
        Substitute => "Substitute",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_plan_creates_file_and_reversible_run() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("Autoagent.toml"),
            crate::config::default_config::default_toml(),
        )
        .unwrap();
        let plan = root.join("p.plan.json");
        std::fs::write(
            &plan,
            r#"{"objective":"demo","summary":"s","files_to_read":[],
          "files_to_create":[{"path":"crates/demo.rs","purpose":"x"}],"files_to_modify":[],
          "operations":[{"kind":"Create","path":"crates/demo.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"// demo"}],
          "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
        )
        .unwrap();
        let run_id = apply(root, &plan, true).unwrap();
        assert!(root.join("crates/demo.rs").as_std_path().exists());
        assert!(root
            .join(format!(".agent/runs/{run_id}/run.json"))
            .as_std_path()
            .exists());
        assert!(root
            .join(format!(".agent/patches/{run_id}.patch"))
            .as_std_path()
            .exists());
    }

    #[test]
    fn apply_without_approval_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("Autoagent.toml"),
            crate::config::default_config::default_toml(),
        )
        .unwrap();
        let plan = root.join("p.plan.json");
        std::fs::write(
            &plan,
            r#"{"objective":"demo","summary":"s","files_to_read":[],
          "files_to_create":[],"files_to_modify":[],
          "operations":[{"kind":"Create","path":"crates/demo.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"x"}],
          "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
        )
        .unwrap();
        let e = apply(root, &plan, false).unwrap_err();
        assert_eq!(e.error_code(), "policy.write_not_approved");
    }
}
