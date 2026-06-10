//! Prompt builder (M3) — embeds the JSON Plan schema and policy constraints so
//! the model self-limits, plus a structural project summary for context. File
//! *contents* are not forwarded in M3 (only metadata), so nothing sensitive
//! leaves the machine here; the planner still validates every returned plan.
//!
//! Two framings are produced (`PromptKind`):
//! - `Project` — planning changes to the *user's* project. The model is told to
//!   author the concrete operations (with real file content) the objective
//!   requires, not to return an empty/timid plan.
//! - `SelfAuthoring` — planning changes to AutoAgent's *own* source (the `evolve`
//!   path). The model is told it is improving AutoAgent itself, to implement the
//!   code change when the objective warrants it, and to ALWAYS include the
//!   `cargo` validation commands so the supervised loop verifies the change.

use crate::analysis::project_analysis::ProjectAnalysis;

/// Which planning posture to prompt for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// Changes to the user's project (the `plan`/`run` path).
    Project,
    /// Changes to AutoAgent's own source (the `evolve` path).
    SelfAuthoring,
}

/// Build the default (`Project`) planning prompt.
pub fn build(objective: &str, analysis: &ProjectAnalysis, recent_decisions: &[String]) -> String {
    build_kind(PromptKind::Project, objective, analysis, recent_decisions)
}

/// Build a planning prompt for the given posture.
pub fn build_kind(
    kind: PromptKind,
    objective: &str,
    analysis: &ProjectAnalysis,
    recent_decisions: &[String],
) -> String {
    let deps = analysis
        .dependencies
        .iter()
        .map(|d| d.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let decisions = if recent_decisions.is_empty() {
        String::new()
    } else {
        format!(
            "\nPrior project decisions (most recent first):\n{}\n",
            recent_decisions
                .iter()
                .map(|d| format!("- {d}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    let role = match kind {
        PromptKind::Project => "You are AutoAgent's planner.",
        PromptKind::SelfAuthoring => {
            "You are AutoAgent improving its OWN source. The workspace IS the AutoAgent \
             codebase (Rust crates under crates/: autoagent-core, autoagent-cli, \
             autoagent-plugin-sdk, autoagent-bingen)."
        }
    };

    // Shared directive: both postures must produce the concrete operations the
    // objective requires — authoring real file `content` — rather than an empty
    // or timid plan. This is what makes AutoAgent actually do the job.
    let authoring_directive = "When the objective requires code changes, IMPLEMENT them: emit the \
         concrete Create/Replace/Write/Append/Rename/Delete operations with the full file `content` \
         needed, following the project's existing conventions. Do NOT return an empty `operations` \
         list when the objective clearly calls for changes. Include `validation_commands` that prove \
         the change (build/test/lint). Only return a minimal plan when the objective is purely \
         informational and needs no edits. Each operation's `kind` MUST be EXACTLY ONE of these \
         literal values — Create, Write, Replace, Append, Delete, Rename, CreateDirectory — never the \
         pipe-joined list itself.";

    let self_directive = match kind {
        PromptKind::Project => String::new(),
        PromptKind::SelfAuthoring =>
            "\n\nSelf-authoring guidance: this objective targets AutoAgent itself. When it names a \
             bug, feature, or improvement to AutoAgent's behavior, author the concrete Rust changes \
             to the relevant crate(s) — new/modified `.rs` files under crates/ — matching existing \
             module structure, error handling, and test conventions. You MUST include these \
             `validation_commands` so the supervised loop verifies the self-change before it is \
             accepted: \"cargo test\", \"cargo clippy --all-targets --all-features -- -D warnings\", \
             \"cargo fmt --all -- --check\". Add a regression test for any bug fix. If the objective \
             is only a question about AutoAgent (no change requested), return a minimal plan with no \
             operations."
                .to_string(),
    };

    format!(
        "{role} Produce ONLY a single JSON object matching this schema:\n\
         {{\"objective\":string,\"summary\":string,\"files_to_read\":[string],\
         \"files_to_create\":[{{\"path\":string,\"purpose\":string}}],\
         \"files_to_modify\":[{{\"path\":string,\"purpose\":string}}],\
         \"operations\":[{{\"kind\":\"Create|Write|Replace|Append|Delete|Rename|CreateDirectory\",\
         \"path\":string,\"destination_path\":string|null,\"reason\":string,\
         \"before_hash\":null,\"after_hash\":null,\"content\":string|null}}],\
         \"validation_commands\":[string],\"risks\":[string],\"rollback_strategy\":\"snapshot\"}}\n\n\
         {authoring_directive}{self_directive}\n\n\
         Constraints: only write under allowed paths; never touch .git, target, .env, SSH material, \
         or any path outside the workspace; rollback_strategy MUST be \"snapshot\".\n\n\
         Project context: language={:?}, dependencies=[{}], top-level dirs={:?}.\n\
         {}\
         Objective: {}\n",
        analysis.language, deps, analysis.top_dirs, decisions, objective
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::project_analysis::{LanguageKind, ProjectAnalysis};

    fn analysis() -> ProjectAnalysis {
        ProjectAnalysis {
            root: "/ws".into(),
            language: LanguageKind::Rust,
            package_manager: None,
            dependencies: vec![],
            file_count: 0,
            source_files: 0,
            top_dirs: vec![],
        }
    }

    #[test]
    fn prompt_embeds_schema_and_objective() {
        let p = build(
            "add a cache",
            &analysis(),
            &["2026-06-08: chose TOML".into()],
        );
        assert!(p.contains("rollback_strategy"));
        assert!(p.contains("add a cache"));
        assert!(p.contains("chose TOML"));
        assert!(p.contains("never touch .git"));
    }

    #[test]
    fn project_prompt_directs_concrete_authoring() {
        let p = build_kind(PromptKind::Project, "add an endpoint", &analysis(), &[]);
        // The general prompt now tells the model to actually implement changes.
        assert!(p.contains("IMPLEMENT them"));
        assert!(p.contains("Do NOT return an empty"));
        // ...but does NOT claim the workspace is AutoAgent itself.
        assert!(!p.contains("improving its OWN source"));
    }

    #[test]
    fn self_authoring_prompt_directs_self_modification_and_validation() {
        let p = build_kind(
            PromptKind::SelfAuthoring,
            "fix the revert bug in autoagent-core",
            &analysis(),
            &[],
        );
        assert!(p.contains("improving its OWN source"));
        assert!(p.contains("crates/"));
        assert!(p.contains("author the concrete Rust changes"));
        // Validation is mandated so each self-change is gated by the run loop.
        assert!(p.contains("cargo test"));
        assert!(p.contains("cargo clippy"));
        assert!(p.contains("regression test for any bug fix"));
    }
}
