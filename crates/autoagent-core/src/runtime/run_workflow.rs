//! Supervised `run` workflow (M4) — plan → apply → validate → bounded repair →
//! report (SPEC-1 FR-20/FR-25). A run is never reported `Completed` while its
//! validation report is failing (SPEC-1 §2.2 reliability).
//!
//! Built on top of the M1 apply loop: each apply/repair iteration is its own
//! reversible run; `run` returns the final run id. (PROPOSED: repairs as linked
//! runs rather than one folder — see the M4 plan's open questions.)

use crate::config::config_schema::AutoAgentConfig;
use crate::error::{AutoAgentError, Result};
use crate::planning::llm::provider::LlmProvider;
use crate::planning::{plan_reader, plan_writer, planner};
use crate::runtime::agent_loop;
use crate::runtime::repair::{RepairContext, StepBudget};
use crate::runtime::run_state::RunState;
use crate::safety::policy_engine::PolicyEngine;
use crate::validation::validation_report::ValidationReport;
use crate::validation::{command_runner, report_md};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RunOutcome {
    pub run_id: String,
    pub final_state: RunState,
    pub report: ValidationReport,
}

/// Run a supervised workflow from an existing plan file (no repair — there is
/// no provider to re-plan with). Used by `run --from`.
pub fn run_with_plan(
    root: &Utf8Path,
    plan_path: &Utf8Path,
    auto_approve: bool,
) -> Result<RunOutcome> {
    let (run_id, report) = apply_and_validate(root, plan_path, auto_approve)?;
    Ok(finish_outcome(run_id, report))
}

/// Run a supervised workflow, generating the plan via `provider` and attempting
/// a bounded repair pass on validation failure. Used by `run "<objective>"`.
pub async fn run_workflow(
    root: &Utf8Path,
    objective: &str,
    provider: &dyn LlmProvider,
    auto_approve: bool,
) -> Result<RunOutcome> {
    let config = AutoAgentConfig::load(root)?;

    let plan = planner::generate_plan(objective, &config, root, provider).await?;
    let (mut run_id, mut report) = apply_written_plan(root, objective, &plan, auto_approve)?;

    if !report.passed {
        let mut budget = StepBudget::new(config.agent.max_steps_per_run);
        while !report.passed && budget.try_consume() {
            // Revert the failed attempt BEFORE re-planning, so each repair starts
            // from the pre-failure tree. Without this, a destructive attempt
            // (e.g. a bad full-file replace) poisons every later repair, which
            // can then never recover. The repair re-plans the whole objective
            // from that clean base with the failure context.
            crate::runtime::revert::revert(root, &run_id)?;

            let ctx = RepairContext::from_failure(&report);
            let repair_objective = format!(
                "{objective}\n\nThe previous attempt failed validation command `{}`:\n{}",
                ctx.failing_command, ctx.error_excerpt
            );
            let repair_plan =
                planner::generate_plan(&repair_objective, &config, root, provider).await?;
            let (rid, rep) = apply_written_plan(root, objective, &repair_plan, auto_approve)?;
            run_id = rid;
            report = rep;
        }
    }

    if report.passed {
        record_decision(root, &config, &run_id, objective);
    }

    Ok(finish_outcome(run_id, report))
}

/// Append a decision summarizing a completed run (best-effort: a memory write
/// failure must not fail an already-completed run).
fn record_decision(root: &Utf8Path, config: &AutoAgentConfig, run_id: &str, objective: &str) {
    let store = crate::memory::memory_store::MemoryStore::new(root.join(&config.memory.directory));
    let _ = store.append_decision(crate::memory::schema::DecisionEntry {
        id: run_id.to_string(),
        date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        decision: objective.to_string(),
        rationale: "supervised run completed".into(),
        run_id: Some(run_id.to_string()),
    });
}

fn apply_written_plan(
    root: &Utf8Path,
    slug_objective: &str,
    plan: &crate::planning::plan::Plan,
    auto_approve: bool,
) -> Result<(String, ValidationReport)> {
    let (json_path, _md) = plan_writer::write_plan(root, &slugify(slug_objective), plan)?;
    apply_and_validate(root, &json_path, auto_approve)
}

/// Apply a plan, run its validation commands, write the report + summary, and
/// correct the run's final state so it never reads `Completed` while failing.
fn apply_and_validate(
    root: &Utf8Path,
    plan_path: &Utf8Path,
    auto_approve: bool,
) -> Result<(String, ValidationReport)> {
    let run_id = agent_loop::apply(root, plan_path, auto_approve)?;

    let config = AutoAgentConfig::load(root)?;
    let real_root = canonical(root);
    let plan = plan_reader::read_plan(plan_path)?;
    let engine = PolicyEngine::from_config(&config, real_root.clone());

    let report = command_runner::run_all(&plan.validation_commands, real_root.clone(), &engine)?;

    let run_dir = real_root.join(&config.runs.directory).join(&run_id);
    std::fs::write(
        run_dir.join("validation-report.md").as_std_path(),
        report_md::render_report(&report),
    )?;
    let state = if report.passed { "Completed" } else { "Failed" };
    std::fs::write(
        run_dir.join("summary.md").as_std_path(),
        format!(
            "# Run Summary\n\n- Objective: {}\n- State: {}\n- Validation: {}\n- Commands: {}\n",
            plan.objective,
            state,
            if report.passed { "passed" } else { "failed" },
            report.commands.len()
        ),
    )?;

    // Correct run.json: apply() wrote Completed/validation_passed=true before
    // validation ran. The workflow owns the final verdict.
    let run_json = run_dir.join("run.json");
    if let Ok(text) = std::fs::read_to_string(run_json.as_std_path()) {
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&text) {
            v["validation_passed"] = json!(report.passed);
            v["state"] = json!(state);
            let out = serde_json::to_string_pretty(&v)
                .map_err(|e| AutoAgentError::Serde(e.to_string()))?;
            std::fs::write(run_json.as_std_path(), out)?;
        }
    }

    Ok((run_id, report))
}

fn finish_outcome(run_id: String, report: ValidationReport) -> RunOutcome {
    let final_state = if report.passed {
        RunState::Completed
    } else {
        RunState::Failed
    };
    RunOutcome {
        run_id,
        final_state,
        report,
    }
}

fn canonical(root: &Utf8Path) -> Utf8PathBuf {
    std::fs::canonicalize(root.as_std_path())
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .unwrap_or_else(|| root.to_path_buf())
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in s.chars().take(40) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let t = out.trim_matches('-').to_string();
    if t.is_empty() {
        "run".into()
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::llm::provider::{LlmProvider, PlanRequest};
    use std::sync::Mutex;

    /// Returns queued responses in order (for the repair sequence).
    struct ScriptedProvider {
        responses: Mutex<Vec<String>>,
    }
    #[async_trait::async_trait]
    impl LlmProvider for ScriptedProvider {
        fn name(&self) -> &str {
            "scripted"
        }
        async fn complete(&self, req: &PlanRequest) -> Result<String> {
            // The planner issues a cheap "scout" call before the plan call; in
            // tests we want no file context, so answer it with an empty list and
            // do not consume a queued plan response.
            if req.context.contains("scoping a code change") {
                return Ok("[]".into());
            }
            let mut r = self.responses.lock().unwrap();
            Ok(if r.is_empty() {
                r.last().cloned().unwrap_or_default()
            } else {
                r.remove(0)
            })
        }
    }

    fn workspace() -> (tempfile::TempDir, AutoAgentConfig) {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("Autoagent.toml"),
            crate::config::default_config::default_toml(),
        )
        .unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"",
        )
        .unwrap();
        let cfg = AutoAgentConfig::load(root).unwrap();
        (dir, cfg)
    }

    fn plan_json(content_path: &str, validation: &str) -> String {
        format!(
            r#"{{"objective":"o","summary":"s","files_to_read":[],
          "files_to_create":[{{"path":"{p}","purpose":"x"}}],"files_to_modify":[],
          "operations":[{{"kind":"Create","path":"{p}","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"// x"}}],
          "validation_commands":[{v}],"risks":[],"rollback_strategy":"snapshot"}}"#,
            p = content_path,
            v = validation
        )
    }

    #[tokio::test]
    async fn run_applies_and_validates_clean() {
        let (dir, _cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let provider = ScriptedProvider {
            responses: Mutex::new(vec![plan_json("crates/x.rs", "")]),
        };
        let outcome = run_workflow(root, "add x", &provider, true).await.unwrap();
        assert!(matches!(outcome.final_state, RunState::Completed));
        assert!(root.join("crates/x.rs").as_std_path().exists());
        let run_dir = root.join(format!(".agent/runs/{}", outcome.run_id));
        assert!(run_dir.join("summary.md").as_std_path().exists());
        assert!(run_dir.join("validation-report.md").as_std_path().exists());
    }

    #[tokio::test]
    async fn run_repairs_after_failing_validation() {
        let (dir, _cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        // Allow `false` so the first plan's validation deterministically fails.
        let mut toml = crate::config::default_config::default_toml();
        toml.push_str("\n# test\n");
        let mut cfg = AutoAgentConfig::from_toml_str(&toml).unwrap();
        cfg.safety.allowed_commands.push("false".into());
        std::fs::write(root.join("Autoagent.toml"), toml::to_string(&cfg).unwrap()).unwrap();

        let provider = ScriptedProvider {
            responses: Mutex::new(vec![
                plan_json("crates/a.rs", "\"false\""), // attempt 1: validation fails
                plan_json("crates/b.rs", ""),          // repair: passes
            ]),
        };
        let outcome = run_workflow(root, "fix", &provider, true).await.unwrap();
        assert!(matches!(outcome.final_state, RunState::Completed));
        assert!(root.join("crates/b.rs").as_std_path().exists());
    }

    /// Regression: a destructive first attempt that corrupts an existing file
    /// AND fails validation must be REVERTED before the repair runs, so the
    /// repair starts from the pre-failure tree and the existing file survives.
    #[tokio::test]
    async fn repair_reverts_destructive_attempt_before_retry() {
        let (dir, _cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let mut cfg =
            AutoAgentConfig::from_toml_str(&crate::config::default_config::default_toml()).unwrap();
        cfg.safety.allowed_commands.push("false".into());
        std::fs::write(root.join("Autoagent.toml"), toml::to_string(&cfg).unwrap()).unwrap();

        // An existing source file that the bad attempt will clobber.
        std::fs::create_dir_all(root.join("src").as_std_path()).unwrap();
        std::fs::write(root.join("src/keep.rs"), "ORIGINAL").unwrap();

        let destructive = r#"{"objective":"o","summary":"s","files_to_read":[],
          "files_to_create":[],"files_to_modify":[{"path":"src/keep.rs","purpose":"x"}],
          "operations":[{"kind":"Replace","path":"src/keep.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"BROKEN"}],
          "validation_commands":["false"],"risks":[],"rollback_strategy":"snapshot"}"#;
        let provider = ScriptedProvider {
            responses: Mutex::new(vec![
                destructive.to_string(), // attempt 1: clobbers keep.rs + fails validation
                plan_json("crates/ok.rs", ""), // repair: clean, passes
            ]),
        };

        let outcome = run_workflow(root, "improve", &provider, true)
            .await
            .unwrap();

        assert!(matches!(outcome.final_state, RunState::Completed));
        assert!(root.join("crates/ok.rs").as_std_path().exists());
        // The clobbered file must be restored to its pre-failure content — the
        // destructive attempt was reverted before the repair, not left in place.
        assert_eq!(
            std::fs::read_to_string(root.join("src/keep.rs")).unwrap(),
            "ORIGINAL",
            "destructive attempt must be reverted before repair"
        );
    }
}
