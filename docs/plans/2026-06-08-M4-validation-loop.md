# M4 — 0.4.0 Validation Loop Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `lore:execute` to implement this plan task-by-task.
> **Scope guard:** Do ONLY what is listed here. If you discover adjacent issues, note them as a TODO and continue. Do NOT fix them.

**Goal:** Add `run "<objective>"` — a supervised one-shot workflow that plans, applies, runs validation commands, and on failure attempts a bounded repair pass, producing a final report.
**Architecture:** `runtime::run_workflow` composes M3's planner, M1's apply loop, and a new `validation::command_runner`. Repair re-invokes the planner with the failure context, bounded by `[agent].max_steps_per_run`. Everything stays snapshotted and reversible (M1 guarantees).
**Tech Stack:** Rust 2021; reuses M1–M3 deps. `command_runner` uses `std::process::Command` through M1's command guard.
**Practices:** TDD, typed-interfaces-first, contract-first.
**Required skills:** none.
**Prerequisite:** **M1** (apply, snapshots, revert, run logger), **M3** (planner) complete.
**Design status:** ⚠️ **PROPOSED DESIGN.** SPEC-1 §13 names "run command, apply plan, run validation, inspect failures, repair pass, final report" and FR-25 makes the repair pass a COULD bounded by `max_steps_per_run`. The repair strategy, failure-context format, and report layout below are design decisions to confirm.

**Contracts:** reuses M1 `ValidationReport`/`CommandValidationResult` (do not redefine). Introduces `RunOutcome`, `RepairContext`.

---

### Task 1: Command runner (guarded validation execution)

**Files:**
- Create: `crates/autoagent-core/src/validation/command_runner.rs`
- Modify: `crates/autoagent-core/src/validation/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::policy_engine::PolicyEngine;
    use crate::config::config_schema::AutoAgentConfig;
    fn engine() -> PolicyEngine {
        let cfg = AutoAgentConfig::from_toml_str(crate::config::default_config::default_toml().as_str()).unwrap();
        PolicyEngine::from_config(&cfg, ".".into())
    }
    #[test] fn runs_allowed_command_and_captures_output() {
        // "cargo --version" is harmless; add it to a test config's allowed_commands
        let r = run_one("cargo --version", ".".into(), &allow(["cargo --version"])).unwrap();
        assert_eq!(r.exit_code, Some(0));
        assert!(r.stdout.contains("cargo"));
    }
    #[test] fn blocked_command_is_policy_error() {
        assert!(run_one("sudo rm -rf /", ".".into(), &engine()).is_err());
    }
}
```
(`allow([...])` builds a `PolicyEngine` whose allowed_commands contain the given entries.)

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation**
```rust
use crate::error::{AutoAgentError, Result};
use crate::validation::validation_report::CommandValidationResult;
use crate::safety::policy_engine::PolicyEngine;
use camino::Utf8PathBuf;
use std::time::Instant;

pub fn run_one(cmd: &str, cwd: Utf8PathBuf, engine: &PolicyEngine) -> Result<CommandValidationResult> {
    engine.check_command(cmd)?;            // policy error bubbles up; never runs a blocked command
    let started = Instant::now();
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let output = std::process::Command::new(parts[0])
        .args(&parts[1..]).current_dir(cwd.as_std_path())
        .output().map_err(|e| AutoAgentError::Validation(format!("{cmd}: {e}")))?;
    Ok(CommandValidationResult {
        command: cmd.to_string(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        duration_ms: started.elapsed().as_millis(),
    })
}

pub fn run_all(cmds: &[String], cwd: Utf8PathBuf, engine: &PolicyEngine)
    -> Result<crate::validation::validation_report::ValidationReport> {
    let mut results = Vec::new();
    for c in cmds { results.push(run_one(c, cwd.clone(), engine)?); }
    let passed = results.iter().all(|r| r.exit_code == Some(0));
    Ok(crate::validation::validation_report::ValidationReport { passed, commands: results })
}
```

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(validation): guarded command runner + report builder"`

---

### Task 2: Validation report Markdown writer

**Files:**
- Create: `crates/autoagent-core/src/validation/report_md.rs`
- Modify: `crates/autoagent-core/src/validation/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::validation_report::*;
    #[test] fn renders_pass_fail_table() {
        let rep = ValidationReport { passed:false, commands: vec![
            CommandValidationResult{command:"cargo build".into(),exit_code:Some(0),stdout:"".into(),stderr:"".into(),duration_ms:10},
            CommandValidationResult{command:"cargo test".into(),exit_code:Some(101),stdout:"".into(),stderr:"boom".into(),duration_ms:20},
        ]};
        let md = render_report(&rep);
        assert!(md.contains("FAIL"));
        assert!(md.contains("cargo test"));
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `render_report(&ValidationReport) -> String`: a header line (`PASSED`/`FAILED`), a table of command | exit | duration | status, and for failed commands a fenced `stderr` excerpt (truncated to N lines). Written to `runs/<id>/validation-report.md` by the run workflow.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(validation): validation-report.md writer"`

---

### Task 3: Repair context + bounded repair strategy

**Files:**
- Create: `crates/autoagent-core/src/runtime/repair.rs`
- Modify: `crates/autoagent-core/src/runtime/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn repair_context_summarizes_failures() {
        let rep = failed_report_with(["cargo test", "error[E0599]: no method `foo`"]);
        let ctx = RepairContext::from_failure(&rep);
        assert!(ctx.failing_command.contains("cargo test"));
        assert!(ctx.error_excerpt.contains("E0599"));
    }
    #[test] fn budget_decrements_and_stops() {
        let mut b = StepBudget::new(2);
        assert!(b.try_consume()); assert!(b.try_consume());
        assert!(!b.try_consume());   // exhausted at max_steps_per_run
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation**
```rust
use crate::validation::validation_report::ValidationReport;

pub struct RepairContext { pub failing_command: String, pub error_excerpt: String }
impl RepairContext {
    pub fn from_failure(rep: &ValidationReport) -> Self {
        let failed = rep.commands.iter().find(|c| c.exit_code != Some(0));
        match failed {
            Some(c) => Self {
                failing_command: c.command.clone(),
                error_excerpt: tail_lines(&format!("{}\n{}", c.stdout, c.stderr), 40),
            },
            None => Self { failing_command: String::new(), error_excerpt: String::new() },
        }
    }
}

pub struct StepBudget { remaining: u32 }
impl StepBudget {
    pub fn new(max: u32) -> Self { Self { remaining: max } }
    pub fn try_consume(&mut self) -> bool {
        if self.remaining == 0 { false } else { self.remaining -= 1; true }
    }
}

fn tail_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}
```

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(runtime): repair context + step budget"`

---

### Task 4: `run` workflow orchestration

**Files:**
- Create: `crates/autoagent-core/src/runtime/run_workflow.rs`
- Modify: `crates/autoagent-core/src/runtime/mod.rs`

**Step 1: Write the failing test** (deterministic — fake provider, validation that passes):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test] async fn run_applies_and_validates() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("Autoagent.toml"), crate::config::default_config::default_toml()).unwrap();
        let provider = fake_provider_creating_file_under_crates();   // returns valid Plan
        let outcome = run_workflow(root, "add x", &provider, /*auto_approve=*/true).await.unwrap();
        assert!(matches!(outcome.final_state, crate::runtime::run_state::RunState::Completed));
        assert!(root.join("crates/x.rs").exists());
        assert!(root.join(format!(".agent/runs/{}/summary.md", outcome.run_id)).exists());
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `run_workflow(root, objective, &provider, auto_approve)`:
1. `generate_plan` (M3) → `validate_plan` (M1).
2. apply via M1 `agent_loop` internals (snapshot → apply → patch) under the approval gate.
3. `command_runner::run_all(plan.validation_commands)`.
4. If `report.passed` → state `Completed`. Else, while `budget.try_consume()`: build `RepairContext`, ask planner for a repair plan, validate+apply, re-validate; stop on pass or budget exhaustion (state `Failed` if still failing — never reported `Completed` with a failing report, per SPEC-1 §2.2 reliability).
5. Write `summary.md` + `run.json` (state) + `validation-report.md`. Return `RunOutcome { run_id, final_state, report }`.
Repairs reuse the SAME run folder, appending events (`run` is one supervised run with possibly multiple apply phases).

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(runtime): supervised run workflow with bounded repair"`

---

### Task 5: `run` CLI command + E2E (repair path with real cargo)

**Files:**
- Create: `crates/autoagent-cli/src/commands/run.rs`
- Modify: `crates/autoagent-cli/src/main.rs` (`Run { objective }`)
- Create: `crates/autoagent-cli/tests/e2e_run.rs`

**Step 1: Write the failing E2E** — a **genuine E2E** using `--from` to supply a deterministic plan (no live LLM) whose `validation_commands` run real `cargo build` against a real generated crate, proving plan→apply→validate end-to-end through the binary:
```rust
use std::process::Command;
fn bin() -> &'static str { env!("CARGO_BIN_EXE_autoagent") }
#[test] fn run_apply_then_real_validation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new(bin()).args(["--yes","init"]).current_dir(root).output().unwrap();
    // a plan that writes a tiny valid Rust file and validates with a real shell-free command
    std::fs::write(root.join("p.plan.json"), r#"{"objective":"touch","summary":"s","files_to_read":[],
      "files_to_create":[{"path":"crates/ok.rs","purpose":"x"}],"files_to_modify":[],
      "operations":[{"kind":"Create","path":"crates/ok.rs","destination_path":null,"reason":"r",
        "before_hash":null,"after_hash":null,"content":"pub fn ok() -> i32 { 1 }\n"}],
      "validation_commands":["cargo --version"],"risks":[],"rollback_strategy":"snapshot"}"#).unwrap();
    // ensure cargo --version is allowed for the test workspace
    // (init writes default config; append the allowed command via `config` or pre-seed toml)
    let out = Command::new(bin()).args(["--yes","run","--from","p.plan.json","touch"])
        .current_dir(root).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(root.join("crates/ok.rs").exists());
    let runs = std::fs::read_dir(root.join(".agent/runs")).unwrap().next().unwrap().unwrap();
    assert!(runs.path().join("validation-report.md").exists());
}
```
> NOTE during execution: `run` must accept `--from` like `plan`/`apply` so the E2E is deterministic without a live model. Add the flag.

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `Run` subcommand: build provider (or `--from` plan), call `run_workflow` on a tokio runtime, print outcome + report path, exit-code-map failures.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(cli): run command + e2e (apply→real validation)"`

---

### Task 6: Quality gate + M4 exit

**Step 1:** fmt + clippy + `cargo test --workspace` green.
**Step 2: Verify M4 exit criteria (SPEC-1 §5):** `run "<objective>"` completes a supervised workflow; failed validation triggers a bounded repair attempt (unit test forces a failing-then-passing provider sequence and asserts ≤ `max_steps_per_run` repairs); result is reversible (revert the run, confirm tree restored).
**Step 3: Commit** → `git add -A && git commit -m "chore(0.4.0): validation loop milestone exit"`

---

## Open design questions (resolve during execution)
- Repair re-planning prompt: pass full file content or just the diff + error? (current: error excerpt + failing command; confirm.)
- Whether a repair that introduces a NEW failing command counts against the same budget (current: yes, single shared budget).
