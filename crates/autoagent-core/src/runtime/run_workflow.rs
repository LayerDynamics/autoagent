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
use crate::planning::prompt_builder::PromptKind;
use crate::planning::{agent_planner, plan_reader, plan_writer};
use crate::runtime::agent_loop;
use crate::runtime::repair::StepBudget;
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
    /// The id of the reproducible session this run recorded (present on a
    /// completed `run`), which `run --replay <id>` re-applies deterministically.
    #[serde(default)]
    pub session_id: Option<String>,
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
    // One shared budget bounds ALL plan→apply work for this run — repairs and
    // (in autonomous mode) forward steps alike — so a run can never exceed
    // `max_steps_per_run` cycles regardless of mode.
    let mut budget = StepBudget::new(config.agent.max_steps_per_run);

    // Cycle 1: plan the objective and apply it, repairing on validation failure.
    let plan = agent_planner::generate_plan_agentic(
        PromptKind::Project,
        objective,
        &config,
        root,
        provider,
        auto_approve,
    )
    .await?;
    let (mut run_id, mut report) = apply_written_plan(root, objective, &plan, auto_approve)?;
    let mut history = vec![plan.summary.clone()];
    // The ordered plans whose changes are retained in the final tree — the
    // reproducible record this run records as a replayable session.
    let mut applied_plans: Vec<crate::planning::plan::Plan> = vec![plan.clone()];

    if !report.passed {
        let (rid, rep, repaired) = repair_to_pass(
            root,
            objective,
            &config,
            provider,
            auto_approve,
            run_id,
            report,
            &mut budget,
        )
        .await?;
        run_id = rid;
        report = rep;
        // The failed cycle-1 plan was reverted; its retained replacement (if the
        // repair landed) is the passing repair plan.
        applied_plans.clear();
        if let Some(p) = repaired {
            applied_plans.push(p);
        }
    }

    // Bounded autonomous continuation (opt-in). Keep performing the NEXT concrete
    // step toward the SAME objective until the model reports it complete, a step
    // cannot be repaired, or the shared budget is exhausted. Every step still goes
    // through plan → policy → apply → validate and is reversible; the objective
    // never changes and no gate is bypassed.
    if config.agent.autonomous && report.passed {
        let real_root = canonical(root);
        let mut touched: std::collections::BTreeSet<String> = op_paths(&plan);
        let mut seen_ops: std::collections::HashSet<String> = op_signatures(&plan);
        while budget.try_consume() {
            // Grounded completion check: show the model the CURRENT contents of the
            // files worked on and ask, strictly, whether the objective is met —
            // so it stops when the work is already there instead of over-producing.
            // Any error/ambiguity defaults to "complete" (stop); never loops forever.
            let state = read_files(&real_root, &touched);
            if objective_complete(objective, &history, &state, provider).await {
                break;
            }
            let next_objective = continuation_objective(objective, &history);
            let next_plan = agent_planner::generate_plan_agentic(
                PromptKind::Project,
                &next_objective,
                &config,
                root,
                provider,
                auto_approve,
            )
            .await?;
            // No-progress backstop: if the next step only re-proposes work already
            // applied (no new operation), the agent is churning — stop.
            let next_sigs = op_signatures(&next_plan);
            if next_sigs.is_subset(&seen_ops) {
                break;
            }
            let (rid, rep) = apply_written_plan(root, objective, &next_plan, auto_approve)?;
            run_id = rid;
            report = rep;
            // The retained plan for this step is the forward plan, unless it was
            // repaired (revert + re-plan), in which case it is the repair plan.
            let mut step_plan = next_plan.clone();
            if !report.passed {
                let (rid, rep, repaired) = repair_to_pass(
                    root,
                    objective,
                    &config,
                    provider,
                    auto_approve,
                    run_id,
                    report,
                    &mut budget,
                )
                .await?;
                run_id = rid;
                report = rep;
                if !report.passed {
                    break; // could not land this step; stop autonomous progress
                }
                if let Some(p) = repaired {
                    step_plan = p;
                }
            }
            applied_plans.push(step_plan);
            seen_ops.extend(next_sigs);
            touched.extend(op_paths(&next_plan));
            history.push(next_plan.summary.clone());
        }
    }

    // Record the reproducible session (best-effort: a session-write failure must
    // not fail an already-completed run).
    let session_id = if report.passed && !applied_plans.is_empty() {
        crate::runtime::session::record(root, &config, objective, &applied_plans).ok()
    } else {
        None
    };

    if report.passed {
        record_decision(root, &config, &run_id, objective);
    }

    Ok(finish_outcome_with_session(run_id, report, session_id))
}

/// Revert-and-re-plan repair loop, shared by the first cycle and (in autonomous
/// mode) each forward step. Consumes the shared step budget.
#[allow(clippy::too_many_arguments)]
async fn repair_to_pass(
    root: &Utf8Path,
    objective: &str,
    config: &AutoAgentConfig,
    provider: &dyn LlmProvider,
    auto_approve: bool,
    mut run_id: String,
    mut report: ValidationReport,
    budget: &mut StepBudget,
) -> Result<(
    String,
    ValidationReport,
    Option<crate::planning::plan::Plan>,
)> {
    let real_root = canonical(root);
    // The repair plan whose changes are retained (the last one applied). Used to
    // record the reproducible session faithfully.
    let mut applied: Option<crate::planning::plan::Plan> = None;
    while !report.passed && budget.try_consume() {
        // Capture what the failed attempt authored BEFORE reverting it, so the
        // repair can fix the specific defect instead of re-guessing from scratch.
        let run_dir = real_root.join(&config.runs.directory).join(&run_id);
        let prior_files = read_authored_files(&run_dir);

        // Revert the failed attempt so the repair starts from the pre-failure
        // tree — a destructive attempt must not poison later repairs.
        crate::runtime::revert::revert(root, &run_id)?;

        let repair_objective = build_repair_objective(objective, &report, &prior_files);
        let repair_plan = agent_planner::generate_plan_agentic(
            PromptKind::Project,
            &repair_objective,
            config,
            root,
            provider,
            auto_approve,
        )
        .await?;
        let (rid, rep) = apply_written_plan(root, objective, &repair_plan, auto_approve)?;
        run_id = rid;
        report = rep;
        applied = Some(repair_plan);
    }
    Ok((run_id, report, applied))
}

/// Prompt for the next autonomous step: pursue the SAME objective. The agent
/// never invents a new goal — it only continues toward the one it was given.
fn continuation_objective(objective: &str, history: &[String]) -> String {
    let done = history
        .iter()
        .map(|h| format!("- {h}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{objective}\n\nSteps already applied and validated toward this objective:\n{done}\n\n\
         Perform the SINGLE next concrete step toward the SAME objective — do NOT start a different \
         or new task."
    )
}

/// The set of paths a plan touches (for tracking what's been worked on).
fn op_paths(plan: &crate::planning::plan::Plan) -> std::collections::BTreeSet<String> {
    plan.operations.iter().map(|o| o.path.to_string()).collect()
}

/// A content signature per operation, so re-proposing the same work is detected.
fn op_signatures(plan: &crate::planning::plan::Plan) -> std::collections::HashSet<String> {
    plan.operations
        .iter()
        .map(|o| {
            let payload = format!(
                "{}{}",
                o.content.as_deref().unwrap_or(""),
                o.anchor.as_deref().unwrap_or("")
            );
            format!(
                "{}:{}:{}",
                agent_loop::kind_str(&o.kind),
                o.path,
                crate::editing::snapshot_manager::sha256_hex(payload.as_bytes())
            )
        })
        .collect()
}

/// Read the current contents of `paths` within the workspace (bounded), so the
/// completion check is grounded in what actually exists now.
fn read_files(
    real_root: &Utf8Path,
    paths: &std::collections::BTreeSet<String>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for p in paths.iter().take(20) {
        let abs = real_root.join(p);
        if let Ok(c) = std::fs::read_to_string(abs.as_std_path()) {
            if c.len() <= 16 * 1024 {
                out.push((p.clone(), c));
            }
        }
    }
    out
}

/// Ask the model whether the objective is satisfied, GROUNDED in the current
/// contents of the files worked on, with a strict "do not invent extra work"
/// rubric. Defaults to `true` (stop) on any error/ambiguity so autonomous mode
/// can never loop forever. This is only a *stop* signal — it starts no new work.
async fn objective_complete(
    objective: &str,
    history: &[String],
    state: &[(String, String)],
    provider: &dyn crate::planning::llm::provider::LlmProvider,
) -> bool {
    use crate::planning::llm::provider::PlanRequest;
    let done = history
        .iter()
        .map(|h| format!("- {h}"))
        .collect::<Vec<_>>()
        .join("\n");
    let files = if state.is_empty() {
        "(no files yet)".to_string()
    } else {
        state
            .iter()
            .map(|(p, c)| format!("=== {p} (current contents) ===\n{c}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let prompt = format!(
        "Autonomous completion check.\n\nObjective: {objective}\n\nSteps already applied:\n{done}\n\n\
         {files}\n\nDecide STRICTLY: the objective is COMPLETE unless a SPECIFIC piece it \
         explicitly requires is still MISSING from the files above. Do NOT invent extra functions, \
         tests, or improvements that the objective did not ask for. If every explicitly-required \
         piece is already present, you are done. Reply with ONLY {{\"complete\": true}} or \
         {{\"complete\": false}}."
    );
    let schema = json!({
        "type": "object",
        "properties": {"complete": {"type": "boolean"}},
        "required": ["complete"]
    });
    let raw = match provider
        .complete(&PlanRequest {
            objective: objective.to_string(),
            context: prompt,
            format: Some(schema),
        })
        .await
    {
        Ok(r) => r,
        Err(_) => return true,
    };
    crate::planning::planner::extract_json(&raw)
        .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
        .and_then(|v| v.get("complete").and_then(|c| c.as_bool()))
        .unwrap_or(true)
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

/// Read the file operations an attempt authored (path + content) from its run
/// folder, so a repair can see and correct its own prior output.
fn read_authored_files(run_dir: &Utf8Path) -> Vec<(String, String)> {
    let text = match std::fs::read_to_string(run_dir.join("file-operations.json").as_std_path()) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let ops: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    ops.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|o| {
                    let p = o.get("path")?.as_str()?.to_string();
                    let c = o.get("content")?.as_str()?.to_string();
                    Some((p, c))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Build a repair prompt that hands the model the FULL failing-validation output
/// plus its own previous authored content, so it makes a targeted fix instead of
/// re-guessing the whole change.
fn build_repair_objective(
    objective: &str,
    report: &ValidationReport,
    prior_files: &[(String, String)],
) -> String {
    let failures = report
        .commands
        .iter()
        .filter(|c| c.exit_code != Some(0))
        .map(|c| {
            format!(
                "$ {} (exit {:?})\n{}\n{}",
                c.command, c.exit_code, c.stdout, c.stderr
            )
        })
        .collect::<Vec<_>>()
        .join("\n---\n");

    let prior = if prior_files.is_empty() {
        String::new()
    } else {
        let body = prior_files
            .iter()
            .map(|(p, c)| format!("=== {p} (your previous attempt) ===\n{c}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        format!("\n\nWhat your previous attempt wrote (now reverted):\n{body}")
    };

    format!(
        "{objective}\n\nYour previous attempt FAILED validation. Fix the SPECIFIC problem shown \
         below — keep what was correct and change only what the failure requires.\n\nFailing \
         validation output:\n{failures}{prior}\n\nReturn a corrected plan that makes validation pass."
    )
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

    // Authoritative validation: ALWAYS run the project's configured correctness
    // gate (build + test) in addition to whatever the model proposed, so a plan
    // that omits or under-specifies validation can never be reported Completed
    // on an unverified change. Config commands are policy-filtered (silently
    // skipped if not allowed) since they are system-added, not model-requested.
    let commands = authoritative_commands(&real_root, &config, &plan.validation_commands, &engine);
    let mut report = command_runner::run_all(&commands, real_root.clone(), &engine, auto_approve)?;

    // Deterministic auto-heal: mechanical failures (formatting, autofixable
    // lints) are fixed by the trusted runtime — no model round-trip — then the
    // run's recorded hashes are refreshed (so revert stays correct) and the
    // suite is re-validated. This eliminates a whole class of repair iterations.
    if !report.passed && auto_heal(&real_root, &report) {
        rerecord_after(&real_root, &config, &run_id)?;
        report = command_runner::run_all(&commands, real_root.clone(), &engine, auto_approve)?;
    }

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

/// The validation set actually run: the project's configured correctness gate
/// (build + test, when non-empty and policy-allowed) followed by the model's
/// proposed commands, de-duplicated. This makes "Completed" mean the project's
/// real build/test passed, regardless of what the plan asked for.
fn authoritative_commands(
    root: &Utf8Path,
    config: &AutoAgentConfig,
    plan_cmds: &[String],
    engine: &PolicyEngine,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in [&config.commands.build, &config.commands.test] {
        let c = c.trim().to_string();
        if !c.is_empty()
            && !out.contains(&c)
            && command_applicable(&c, root)
            && engine.check_command(&c).is_ok()
        {
            out.push(c);
        }
    }
    for c in plan_cmds {
        if !out.contains(c) {
            out.push(c.clone());
        }
    }
    out
}

/// Whether a configured command can run in this workspace. A `cargo` command
/// needs a `Cargo.toml`; otherwise it would error on a non-Rust project and turn
/// every run into a false failure. Non-cargo commands are assumed applicable.
fn command_applicable(cmd: &str, root: &Utf8Path) -> bool {
    let c = cmd.trim_start();
    if c == "cargo" || c.starts_with("cargo ") {
        root.join("Cargo.toml").as_std_path().exists()
    } else {
        true
    }
}

/// Deterministically fix mechanical validation failures (formatting, autofixable
/// lints) using the trusted toolchain. Returns true if any fixer ran to success.
/// Model-authored content is never required; these are reversible source edits.
fn auto_heal(real_root: &Utf8Path, report: &ValidationReport) -> bool {
    let mut healed = false;
    for c in report.commands.iter().filter(|c| c.exit_code != Some(0)) {
        let cmd = c.command.to_lowercase();
        if cmd.contains("fmt") {
            healed |= run_fixer(real_root, &["fmt", "--all"]);
        } else if cmd.contains("clippy") {
            healed |= run_fixer(
                real_root,
                &[
                    "clippy",
                    "--fix",
                    "--allow-dirty",
                    "--allow-no-vcs",
                    "--all-targets",
                ],
            );
        }
    }
    healed
}

fn run_fixer(real_root: &Utf8Path, args: &[&str]) -> bool {
    std::process::Command::new("cargo")
        .args(args)
        .current_dir(real_root.as_std_path())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// After an auto-heal mutates applied files, refresh each run-tracked file's
/// `after_hash` in run.json so `revert`'s drift check still matches and the run
/// stays fully reversible.
fn rerecord_after(real_root: &Utf8Path, config: &AutoAgentConfig, run_id: &str) -> Result<()> {
    let run_json = real_root
        .join(&config.runs.directory)
        .join(run_id)
        .join("run.json");
    let text = match std::fs::read_to_string(run_json.as_std_path()) {
        Ok(t) => t,
        Err(_) => return Ok(()),
    };
    let mut v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| AutoAgentError::Serde(e.to_string()))?;
    if let Some(files) = v.get_mut("files_modified").and_then(|f| f.as_array_mut()) {
        for f in files.iter_mut() {
            if let Some(path) = f.get("path").and_then(|p| p.as_str()) {
                let abs = real_root.join(path);
                if abs.as_std_path().is_file() {
                    if let Ok(bytes) = std::fs::read(abs.as_std_path()) {
                        f["after_hash"] =
                            json!(crate::editing::snapshot_manager::sha256_hex(&bytes));
                    }
                }
            }
        }
    }
    let out = serde_json::to_string_pretty(&v).map_err(|e| AutoAgentError::Serde(e.to_string()))?;
    std::fs::write(run_json.as_std_path(), out)?;
    Ok(())
}

fn finish_outcome(run_id: String, report: ValidationReport) -> RunOutcome {
    finish_outcome_with_session(run_id, report, None)
}

fn finish_outcome_with_session(
    run_id: String,
    report: ValidationReport,
    session_id: Option<String>,
) -> RunOutcome {
    let final_state = if report.passed {
        RunState::Completed
    } else {
        RunState::Failed
    };
    RunOutcome {
        run_id,
        final_state,
        report,
        session_id,
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
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        // A buildable crate so the now-authoritative `cargo build`/`cargo test`
        // validation actually has targets and passes for a clean change.
        std::fs::create_dir_all(root.join("src").as_std_path()).unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
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

    #[test]
    fn authoritative_validation_always_includes_build_and_test() {
        let (dir, cfg) = workspace(); // has a Cargo.toml -> cargo commands apply
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let engine = PolicyEngine::from_config(&cfg, root.to_path_buf());
        // Model proposed only a test command; build must still be added, and the
        // duplicate `cargo test` must not appear twice.
        let cmds = authoritative_commands(root, &cfg, &["cargo test".to_string()], &engine);
        assert!(cmds.contains(&"cargo build".to_string()), "must add build");
        assert_eq!(
            cmds.iter().filter(|c| *c == "cargo test").count(),
            1,
            "no duplicate test command"
        );
        // A model command not in the configured gate is preserved.
        let cmds2 = authoritative_commands(root, &cfg, &["cargo doc".to_string()], &engine);
        assert!(cmds2.contains(&"cargo build".to_string()));
        assert!(cmds2.contains(&"cargo test".to_string()));
        assert!(cmds2.contains(&"cargo doc".to_string()));
    }

    #[test]
    fn authoritative_validation_skips_cargo_without_manifest() {
        // A non-Rust workspace (no Cargo.toml) must NOT have cargo build/test
        // forced onto it — that would turn every run into a false failure.
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("Autoagent.toml"),
            crate::config::default_config::default_toml(),
        )
        .unwrap();
        let cfg = AutoAgentConfig::load(root).unwrap();
        let engine = PolicyEngine::from_config(&cfg, root.to_path_buf());
        let cmds = authoritative_commands(root, &cfg, &["git status".to_string()], &engine);
        assert_eq!(
            cmds,
            vec!["git status".to_string()],
            "no cargo without Cargo.toml"
        );
    }

    #[test]
    fn repair_objective_includes_full_error_and_prior_attempt() {
        use crate::validation::validation_report::CommandValidationResult;
        let report = ValidationReport {
            passed: false,
            commands: vec![CommandValidationResult {
                command: "cargo test".into(),
                exit_code: Some(101),
                stdout: "running 1 test".into(),
                stderr: "assertion `left == right` failed\n  left: \"wor\"\n right: \"worl\""
                    .into(),
                duration_ms: 5,
            }],
        };
        let prior = vec![("src/x.rs".to_string(), "pub fn f() -> u8 { 1 }".to_string())];
        let obj = build_repair_objective("add the thing", &report, &prior);
        assert!(obj.contains("add the thing"));
        assert!(obj.contains("cargo test"));
        // The FULL failing output is included (not a 40-line excerpt) so the model
        // can see the exact assertion mismatch.
        assert!(obj.contains("left: \"wor\""));
        assert!(obj.contains("right: \"worl\""));
        // And the prior attempt's own content, so it can correct itself.
        assert!(obj.contains("src/x.rs (your previous attempt)"));
        assert!(obj.contains("pub fn f() -> u8 { 1 }"));
    }

    /// Auto-heal: a mechanical formatting failure is fixed by the trusted
    /// toolchain (cargo fmt) without a model round-trip, and the run reaches
    /// Completed with the file reformatted. Runs real rustfmt.
    #[test]
    fn auto_heal_fixes_formatting_and_reaches_completed() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("Autoagent.toml"),
            crate::config::default_config::default_toml(),
        )
        .unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"demo\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src").as_std_path()).unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub mod messy;\n").unwrap();
        let plan = root.join("p.json");
        std::fs::write(
            &plan,
            r#"{"objective":"o","summary":"s","files_to_read":[],
              "files_to_create":[{"path":"src/messy.rs","purpose":"x"}],"files_to_modify":[],
              "operations":[{"kind":"Create","path":"src/messy.rs","destination_path":null,
                "reason":"r","before_hash":null,"after_hash":null,"content":"pub fn  add( a:i32 )->i32{a+1}\n"}],
              "validation_commands":["cargo fmt --all -- --check"],"risks":[],"rollback_strategy":"snapshot"}"#,
        )
        .unwrap();
        let outcome = run_with_plan(
            root,
            camino::Utf8Path::from_path(plan.as_std_path()).unwrap(),
            true,
        )
        .unwrap();
        assert!(
            matches!(outcome.final_state, RunState::Completed),
            "auto-heal (cargo fmt) should make the fmt --check validation pass"
        );
        let healed = std::fs::read_to_string(root.join("src/messy.rs")).unwrap();
        assert!(
            healed.contains("pub fn add(a: i32) -> i32 {"),
            "messy.rs should have been reformatted by auto-heal, got:\n{healed}"
        );
    }

    /// Scripts plan responses + completion-check answers separately, so an
    /// autonomous run can be driven deterministically.
    struct AutonomousProvider {
        plans: Mutex<Vec<String>>,
        completes: Mutex<Vec<bool>>,
    }
    #[async_trait::async_trait]
    impl LlmProvider for AutonomousProvider {
        fn name(&self) -> &str {
            "autonomous"
        }
        async fn complete(&self, req: &PlanRequest) -> Result<String> {
            if req.context.contains("scoping a code change") {
                return Ok("[]".into()); // scout
            }
            if req.context.contains("Autonomous completion check") {
                let mut c = self.completes.lock().unwrap();
                let done = if c.is_empty() { true } else { c.remove(0) };
                return Ok(format!("{{\"complete\": {done}}}"));
            }
            Ok(self.plans.lock().unwrap().remove(0)) // plan
        }
    }

    #[tokio::test]
    async fn autonomous_mode_runs_multiple_steps_until_complete() {
        let (dir, _cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let mut cfg =
            AutoAgentConfig::from_toml_str(&crate::config::default_config::default_toml()).unwrap();
        cfg.agent.autonomous = true;
        std::fs::write(root.join("Autoagent.toml"), toml::to_string(&cfg).unwrap()).unwrap();

        // Step 1 → a.rs; "not complete" → step 2 → b.rs; "complete" → stop.
        let provider = AutonomousProvider {
            plans: Mutex::new(vec![
                plan_json("crates/a.rs", ""),
                plan_json("crates/b.rs", ""),
            ]),
            completes: Mutex::new(vec![false, true]),
        };
        let outcome = run_workflow(root, "create two modules", &provider, true)
            .await
            .unwrap();
        assert!(matches!(outcome.final_state, RunState::Completed));
        // BOTH forward steps were applied autonomously toward the one objective.
        assert!(
            root.join("crates/a.rs").as_std_path().exists(),
            "step 1 applied"
        );
        assert!(
            root.join("crates/b.rs").as_std_path().exists(),
            "step 2 applied"
        );
    }

    #[tokio::test]
    async fn non_autonomous_stops_after_one_successful_step() {
        let (dir, _cfg) = workspace(); // default config: autonomous = false
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        // Even though a second step is queued, the non-autonomous run must stop
        // after the first successful cycle.
        let provider = ScriptedProvider {
            responses: Mutex::new(vec![
                plan_json("crates/a.rs", ""),
                plan_json("crates/b.rs", ""),
            ]),
        };
        let outcome = run_workflow(root, "one step", &provider, true)
            .await
            .unwrap();
        assert!(matches!(outcome.final_state, RunState::Completed));
        assert!(root.join("crates/a.rs").as_std_path().exists());
        assert!(
            !root.join("crates/b.rs").as_std_path().exists(),
            "non-autonomous must not perform a second step"
        );
    }

    #[tokio::test]
    async fn autonomous_stops_on_no_progress_even_if_model_never_says_done() {
        let (dir, _cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let mut cfg =
            AutoAgentConfig::from_toml_str(&crate::config::default_config::default_toml()).unwrap();
        cfg.agent.autonomous = true;
        std::fs::write(root.join("Autoagent.toml"), toml::to_string(&cfg).unwrap()).unwrap();

        // The model NEVER reports completion and keeps re-proposing the SAME op.
        // The deterministic no-progress backstop must stop the run rather than
        // churn through the whole budget (over-production guard).
        let provider = AutonomousProvider {
            plans: Mutex::new(vec![
                plan_json("crates/a.rs", ""),
                plan_json("crates/a.rs", ""), // identical -> no new work
                plan_json("crates/a.rs", ""),
            ]),
            completes: Mutex::new(vec![false, false, false]),
        };
        let outcome = run_workflow(root, "create a.rs", &provider, true)
            .await
            .unwrap();
        assert!(matches!(outcome.final_state, RunState::Completed));
        assert!(root.join("crates/a.rs").as_std_path().exists());
        // Exactly one apply happened — the re-proposed identical step was refused
        // by the no-progress backstop, so the run did not churn.
        let runs = std::fs::read_dir(root.join(".agent/runs"))
            .unwrap()
            .filter_map(|e| e.ok())
            .count();
        assert_eq!(
            runs, 1,
            "no-progress backstop must stop after the first apply"
        );
    }
}
