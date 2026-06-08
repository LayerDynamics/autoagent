//! Plan writers (M3) — serialize a `Plan` to paired `.plan.json` (the machine
//! contract) and `.plan.md` (human-readable) under `.agent/plans/`.

use crate::editing::file_operation::FileOperationKind;
use crate::error::{AutoAgentError, Result};
use crate::planning::plan::Plan;
use camino::{Utf8Path, Utf8PathBuf};
use std::fmt::Write as _;

/// Write `<timestamp>-<slug>.plan.{json,md}`; returns (json_path, md_path).
pub fn write_plan(root: &Utf8Path, slug: &str, plan: &Plan) -> Result<(Utf8PathBuf, Utf8PathBuf)> {
    let dir = root.join(".agent/plans");
    std::fs::create_dir_all(dir.as_std_path())?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let base = format!("{stamp}-{slug}");

    let json_path = dir.join(format!("{base}.plan.json"));
    let json =
        serde_json::to_string_pretty(plan).map_err(|e| AutoAgentError::Serde(e.to_string()))?;
    std::fs::write(json_path.as_std_path(), json)?;

    let md_path = dir.join(format!("{base}.plan.md"));
    std::fs::write(md_path.as_std_path(), render_md(plan))?;

    Ok((json_path, md_path))
}

fn render_md(plan: &Plan) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Plan: {}\n", plan.objective);
    let _ = writeln!(s, "{}\n", plan.summary);

    let _ = writeln!(s, "## Operations\n");
    let _ = writeln!(s, "| Kind | Path | Reason |");
    let _ = writeln!(s, "| --- | --- | --- |");
    for op in &plan.operations {
        let _ = writeln!(
            s,
            "| {} | `{}` | {} |",
            kind_str(&op.kind),
            op.path,
            op.reason
        );
    }
    let _ = writeln!(s);

    let _ = writeln!(s, "## Validation\n");
    if plan.validation_commands.is_empty() {
        let _ = writeln!(s, "_none_\n");
    } else {
        for c in &plan.validation_commands {
            let _ = writeln!(s, "- `{c}`");
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(s, "## Risks\n");
    if plan.risks.is_empty() {
        let _ = writeln!(s, "_none_\n");
    } else {
        for r in &plan.risks {
            let _ = writeln!(s, "- {r}");
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(s, "## Rollback\n");
    let _ = writeln!(s, "{}", plan.rollback_strategy);
    s
}

fn kind_str(kind: &FileOperationKind) -> &'static str {
    use FileOperationKind::*;
    match kind {
        Create => "Create",
        Write => "Write",
        Replace => "Replace",
        Append => "Append",
        Delete => "Delete",
        Rename => "Rename",
        CreateDirectory => "CreateDirectory",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::file_operation::FileOperation;

    fn sample_plan() -> Plan {
        Plan {
            objective: "add cache".into(),
            summary: "introduce a cache layer".into(),
            files_to_read: vec![],
            files_to_create: vec![],
            files_to_modify: vec![],
            operations: vec![FileOperation {
                kind: FileOperationKind::Create,
                path: "crates/cache.rs".into(),
                destination_path: None,
                reason: "new cache".into(),
                before_hash: None,
                after_hash: None,
                content: Some("// cache".into()),
            }],
            validation_commands: vec!["cargo build".into()],
            risks: vec!["none".into()],
            rollback_strategy: "snapshot".into(),
        }
    }

    #[test]
    fn writes_paired_json_and_md() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let plan = sample_plan();
        let (json_path, md_path) = write_plan(root, "add-cache", &plan).unwrap();
        assert!(json_path.as_str().ends_with(".plan.json"));
        assert!(md_path.as_str().ends_with(".plan.md"));

        let md = std::fs::read_to_string(md_path.as_std_path()).unwrap();
        assert!(md.contains("## Operations"));
        assert!(md.contains("crates/cache.rs"));

        // round-trip: the JSON we wrote must re-read as the same Plan
        let reread = crate::planning::plan_reader::read_plan(&json_path).unwrap();
        assert_eq!(reread.objective, plan.objective);
        assert_eq!(reread.operations.len(), 1);
    }
}
