//! Prompt builder (M3) — embeds the JSON Plan schema and policy constraints so
//! the model self-limits, plus a structural project summary for context. File
//! *contents* are not forwarded in M3 (only metadata), so nothing sensitive
//! leaves the machine here; the planner still validates every returned plan.

use crate::analysis::project_analysis::ProjectAnalysis;

pub fn build(objective: &str, analysis: &ProjectAnalysis) -> String {
    let deps = analysis
        .dependencies
        .iter()
        .map(|d| d.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "You are AutoAgent's planner. Produce ONLY a single JSON object matching this schema:\n\
         {{\"objective\":string,\"summary\":string,\"files_to_read\":[string],\
         \"files_to_create\":[{{\"path\":string,\"purpose\":string}}],\
         \"files_to_modify\":[{{\"path\":string,\"purpose\":string}}],\
         \"operations\":[{{\"kind\":\"Create|Write|Replace|Append|Delete|Rename|CreateDirectory\",\
         \"path\":string,\"destination_path\":string|null,\"reason\":string,\
         \"before_hash\":null,\"after_hash\":null,\"content\":string|null}}],\
         \"validation_commands\":[string],\"risks\":[string],\"rollback_strategy\":\"snapshot\"}}\n\n\
         Constraints: only write under allowed paths; never touch .git, target, .env, SSH material, \
         or any path outside the workspace; rollback_strategy MUST be \"snapshot\".\n\n\
         Project context: language={:?}, dependencies=[{}], top-level dirs={:?}.\n\
         Objective: {}\n",
        analysis.language, deps, analysis.top_dirs, objective
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::project_analysis::{LanguageKind, ProjectAnalysis};

    #[test]
    fn prompt_embeds_schema_and_objective() {
        let a = ProjectAnalysis {
            root: "/ws".into(),
            language: LanguageKind::Rust,
            package_manager: None,
            dependencies: vec![],
            file_count: 0,
            source_files: 0,
            top_dirs: vec![],
        };
        let p = build("add a cache", &a);
        assert!(p.contains("rollback_strategy"));
        assert!(p.contains("add a cache"));
        assert!(p.contains("never touch .git"));
    }
}
