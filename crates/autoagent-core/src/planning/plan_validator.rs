//! Plan validator — enforces every SPEC-1 §3.4.1 schema and policy rule before
//! a plan may be applied. A plan failing ANY rule is rejected wholesale.

use crate::editing::file_operation::{FileOperation, FileOperationKind};
use crate::error::{AutoAgentError, Result};
use crate::planning::plan::Plan;
use crate::safety::policy_engine::PolicyEngine;

pub fn validate_plan(plan: &Plan, engine: &PolicyEngine) -> Result<()> {
    if plan.objective.trim().is_empty() {
        return Err(AutoAgentError::Plan("objective is empty".into()));
    }
    if plan.summary.trim().is_empty() {
        return Err(AutoAgentError::Plan("summary is empty".into()));
    }
    if plan.operations.is_empty() {
        return Err(AutoAgentError::Plan("plan has no operations".into()));
    }
    if plan.rollback_strategy != "snapshot" {
        return Err(AutoAgentError::Plan(format!(
            "unsupported rollback_strategy '{}' (only 'snapshot' in 0.1.0)",
            plan.rollback_strategy
        )));
    }
    for (i, op) in plan.operations.iter().enumerate() {
        validate_op(i, op, engine)?;
    }
    for cmd in &plan.validation_commands {
        // Blocked / unsafe commands bubble up as policy errors. An unknown but
        // clean command (CommandNotApproved) is acceptable at plan time — it is
        // resolved by the approval gate when the run executes.
        match engine.check_command(cmd) {
            Ok(_) => {}
            Err(e) if e.error_code() == "policy.command_not_approved" => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn validate_op(i: usize, op: &FileOperation, engine: &PolicyEngine) -> Result<()> {
    use FileOperationKind::*;
    let needs_content = matches!(op.kind, Create | Write | Replace | Append);
    if needs_content && op.content.is_none() {
        return Err(AutoAgentError::Plan(format!(
            "op[{i}] {:?} requires content",
            op.kind
        )));
    }
    if matches!(op.kind, Rename) && op.destination_path.is_none() {
        return Err(AutoAgentError::Plan(format!(
            "op[{i}] Rename requires destination_path"
        )));
    }
    // Every touched path must pass the write policy (blocked/escape bubbles up).
    engine.check_write(op.path.clone())?;
    if let Some(dest) = &op.destination_path {
        engine.check_write(dest.clone())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::config_schema::AutoAgentConfig;
    use crate::config::default_config;
    use crate::editing::file_operation::FileOperation;
    use camino::Utf8PathBuf;

    fn engine() -> PolicyEngine {
        let cfg = AutoAgentConfig::from_toml_str(&default_config::default_toml()).unwrap();
        PolicyEngine::from_config(&cfg, "/ws".into())
    }

    fn op(kind: FileOperationKind, path: &str, content: Option<&str>) -> FileOperation {
        FileOperation {
            kind,
            path: Utf8PathBuf::from(path),
            destination_path: None,
            reason: "r".into(),
            before_hash: None,
            after_hash: None,
            content: content.map(|c| c.to_string()),
        }
    }

    fn minimal_plan(ops: Vec<FileOperation>) -> Plan {
        Plan {
            objective: "o".into(),
            summary: "s".into(),
            files_to_read: vec![],
            files_to_create: vec![],
            files_to_modify: vec![],
            operations: ops,
            validation_commands: vec![],
            risks: vec![],
            rollback_strategy: "snapshot".into(),
        }
    }

    #[test]
    fn accepts_valid_plan() {
        let p = minimal_plan(vec![op(
            FileOperationKind::Create,
            "crates/x.rs",
            Some("x"),
        )]);
        assert!(validate_plan(&p, &engine()).is_ok());
    }

    #[test]
    fn rejects_empty_operations() {
        let p = minimal_plan(vec![]);
        let e = validate_plan(&p, &engine()).unwrap_err();
        assert_eq!(e.error_code(), "plan");
    }

    #[test]
    fn rejects_blocked_write_path() {
        let p = minimal_plan(vec![op(FileOperationKind::Write, ".git/config", Some("x"))]);
        let e = validate_plan(&p, &engine()).unwrap_err();
        assert_eq!(e.error_code(), "policy.blocked_path");
    }

    #[test]
    fn rejects_missing_content() {
        let p = minimal_plan(vec![op(FileOperationKind::Create, "crates/x.rs", None)]);
        assert_eq!(
            validate_plan(&p, &engine()).unwrap_err().error_code(),
            "plan"
        );
    }

    #[test]
    fn rejects_non_snapshot_rollback() {
        let mut p = minimal_plan(vec![op(
            FileOperationKind::Create,
            "crates/x.rs",
            Some("x"),
        )]);
        p.rollback_strategy = "manual".into();
        assert!(validate_plan(&p, &engine()).is_err());
    }
}
