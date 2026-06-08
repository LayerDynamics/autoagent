# M6 — 0.6.0 Evolve Mode Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `lore:execute` to implement this plan task-by-task.
> **Scope guard:** Do ONLY what is listed here. If you discover adjacent issues, note them as a TODO and continue. Do NOT fix them.

**Goal:** Add `evolve "<objective>"` — controlled self-authoring of AutoAgent's own source: self-analysis, self-plan, branch-before-evolve, and (only on explicit apply) self-apply + self-test. **Plan-only by default; gated by `allow_self_modification`.**
**Architecture:** `evolve` is a constrained `run` (M4) where the workspace IS the AutoAgent repo. The `git::branch_manager` creates an isolated branch before any write so self-authoring never touches the checked-out branch. This is the product's marquee identity — "controlled self-authoring, not uncontrolled self-replication" (SPEC-1 §1).
**Tech Stack:** Rust 2021; **new dep** `git2` (SPEC-1 §12 optional) for branch ops, OR shell `git` through the command guard. PROPOSED: use the guarded `git` CLI (no new dep, consistent with the command-guard model).
**Practices:** TDD, typed-interfaces-first, contract-first.
**Required skills:** none.
**Prerequisite:** **M1–M4** complete (apply, planner, run workflow, validation). M5 optional but recommended (self-decisions in memory).
**Design status:** ⚠️ **PROPOSED DESIGN.** SPEC-1 §13 names "self-modification flag, branch-before-evolve, self-analysis, self-plan, self-apply, self-test" and FR-23 fixes the policy (plan-only default, gated). The branch naming, the gate enforcement points, and the self-test selection below are design decisions to confirm. **This milestone carries the highest risk (SPEC-1 R-5); the gate and branch isolation must be verified, not assumed.**

**Contracts:** reuses M4 `run_workflow`, M1 `TaskContext.self_modification`/`AgentMode::Evolve`. Introduces `git::BranchManager`, `EvolveGuard`.

---

### Task 1: Git client (read-only status/diff via guarded CLI)

**Files:**
- Create: `crates/autoagent-core/src/git/git_client.rs`
- Create: `crates/autoagent-core/src/git/mod.rs`

**Step 1: Write the failing test** (uses a real temp git repo)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn init_repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::process::Command::new("git").arg("init").current_dir(d.path()).output().unwrap();
        d
    }
    #[test] fn reports_current_branch() {
        let d = init_repo();
        let root = camino::Utf8Path::from_path(d.path()).unwrap();
        let branch = current_branch(root).unwrap();
        assert!(branch == "main" || branch == "master");
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `current_branch(root)`, `status(root)`, `diff(root)` shelling out to `git` (`rev-parse --abbrev-ref HEAD`, `status --porcelain`, `diff`) with `cwd=root`. These map to SPEC-1's allowed `git status/diff/branch/checkout` commands. Read-only.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(git): read-only git client (branch/status/diff)"`

---

### Task 2: Branch manager (branch-before-evolve)

**Files:**
- Create: `crates/autoagent-core/src/git/branch_manager.rs`
- Modify: `crates/autoagent-core/src/git/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn creates_and_checks_out_evolve_branch() {
        let d = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(d.path()).unwrap();
        std::process::Command::new("git").arg("init").current_dir(root.as_std_path()).output().unwrap();
        // need at least one commit for a branch to exist
        std::fs::write(root.join("seed"), "x").unwrap();
        for a in [["add","-A"].as_slice(), ["commit","-m","seed"].as_slice()] {
            std::process::Command::new("git").args(a).current_dir(root.as_std_path())
                .env("GIT_AUTHOR_NAME","t").env("GIT_AUTHOR_EMAIL","t@t")
                .env("GIT_COMMITTER_NAME","t").env("GIT_COMMITTER_EMAIL","t@t").output().unwrap();
        }
        let branch = branch_before_evolve(root, "20260608T000000Z-x").unwrap();
        assert_eq!(branch, "autoagent/evolve/20260608T000000Z-x");
        assert_eq!(current_branch(root).unwrap(), branch);
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `branch_before_evolve(root, run_id) -> Result<String>`: branch name `autoagent/evolve/<run-id>`; run guarded `git checkout -b <name>`; return the name. If the repo is dirty, refuse (return `AutoAgentError::Workspace`) so evolve starts from a clean tree.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(git): branch-before-evolve isolation"`

---

### Task 3: Evolve guard (enforce gate + plan-only default)

**Files:**
- Create: `crates/autoagent-core/src/runtime/evolve_guard.rs`
- Modify: `crates/autoagent-core/src/runtime/mod.rs`

**Step 1: Write the failing tests** (the safety-critical ones for R-5):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::config_schema::AutoAgentConfig;
    fn cfg_with_self_mod(v: bool) -> AutoAgentConfig {
        let mut c = AutoAgentConfig::from_toml_str(crate::config::default_config::default_toml().as_str()).unwrap();
        c.agent.allow_self_modification = v; c
    }
    #[test] fn blocks_apply_when_self_mod_disabled() {
        let g = EvolveGuard::new(&cfg_with_self_mod(false));
        assert!(g.authorize_apply().is_err());      // default: plan-only, apply refused
    }
    #[test] fn plan_only_always_allowed() {
        assert!(EvolveGuard::new(&cfg_with_self_mod(false)).authorize_plan().is_ok());
    }
    #[test] fn apply_allowed_only_with_flag_and_explicit_apply() {
        let g = EvolveGuard::new(&cfg_with_self_mod(true));
        assert!(g.authorize_apply().is_ok());
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation**
```rust
use crate::config::config_schema::AutoAgentConfig;
use crate::error::{AutoAgentError, Result};

pub struct EvolveGuard { allow_self_mod: bool }
impl EvolveGuard {
    pub fn new(cfg: &AutoAgentConfig) -> Self { Self { allow_self_mod: cfg.agent.allow_self_modification } }
    pub fn authorize_plan(&self) -> Result<()> { Ok(()) }   // planning self is always allowed
    pub fn authorize_apply(&self) -> Result<()> {
        if self.allow_self_mod { Ok(()) }
        else { Err(AutoAgentError::Policy(crate::error::PolicyError::WriteNotApproved(
            "self-modification disabled (allow_self_modification=false); evolve is plan-only".into()))) }
    }
}
```

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(runtime): evolve guard enforcing plan-only default + gate"`

---

### Task 4: `evolve` workflow

**Files:**
- Create: `crates/autoagent-core/src/runtime/evolve_workflow.rs`
- Modify: `crates/autoagent-core/src/runtime/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test] async fn evolve_plan_only_writes_plan_not_source() {
        let (root, _g) = autoagent_repo_fixture();  // a temp git repo with Autoagent.toml, self_mod=false
        let provider = fake_provider_editing_planner_rs();
        let outcome = evolve(root, "improve planner", &provider).await.unwrap();
        assert!(outcome.plan_path.exists());          // plan written
        assert!(!outcome.applied);                    // NOT applied (plan-only default)
        // source file untouched
        assert_eq!(std::fs::read_to_string(root.join("crates/autoagent-core/src/planning/planner.rs")).unwrap(),
                   ORIGINAL_PLANNER);
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `evolve(root, objective, &provider)`:
1. `EvolveGuard::authorize_plan` (always ok).
2. Self-analyze (M2 over AutoAgent's own tree) + load self-memory (M5).
3. Self-plan (M3 planner) → validate (M1, with the same path policy — writes confined to `crates/`, etc.).
4. Write the plan artifacts (M3). **Stop here by default** (`applied=false`).
5. Only if `authorize_apply().is_ok()` AND the caller passed explicit `--apply`: `branch_before_evolve`, then `run_workflow` (M4) on the isolated branch, self-test via `[commands].test`. Set `applied=true`.
Returns `EvolveOutcome { plan_path, applied, branch: Option<String>, run_id: Option<String> }`. `TaskContext.mode = Evolve`, `self_modification` reflects config.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(runtime): evolve workflow (self-analyze→self-plan, gated self-apply)"`

---

### Task 5: `evolve` CLI command + E2E (plan-only safety)

**Files:**
- Create: `crates/autoagent-cli/src/commands/evolve.rs`
- Modify: `crates/autoagent-cli/src/main.rs` (`Evolve { objective, #[arg(long)] apply: bool }`)
- Create: `crates/autoagent-cli/tests/e2e_evolve.rs`

**Step 1: Write the failing E2E** — proves plan-only default does NOT modify source even when `--apply` is passed but `allow_self_modification=false`:
```rust
use std::process::Command;
fn bin() -> &'static str { env!("CARGO_BIN_EXE_autoagent") }
#[test] fn evolve_apply_refused_when_self_mod_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new(bin()).args(["--yes","init"]).current_dir(root).output().unwrap();
    // default config has allow_self_modification=false
    std::fs::create_dir_all(root.join("crates")).unwrap();
    std::fs::write(root.join("crates/keep.rs"), "ORIGINAL").unwrap();
    std::fs::write(root.join("p.plan.json"), r#"{"objective":"self","summary":"s","files_to_read":[],
      "files_to_create":[],"files_to_modify":[{"path":"crates/keep.rs","purpose":"x"}],
      "operations":[{"kind":"Replace","path":"crates/keep.rs","destination_path":null,"reason":"r",
        "before_hash":null,"after_hash":null,"content":"HACKED"}],
      "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#).unwrap();
    let out = Command::new(bin()).args(["evolve","--from","p.plan.json","--apply","self"])
        .current_dir(root).output().unwrap();
    // apply refused → non-zero exit (policy), and the source is UNCHANGED
    assert!(!out.status.success());
    assert_eq!(std::fs::read_to_string(root.join("crates/keep.rs")).unwrap(), "ORIGINAL");
}
```
> Add `--from` to `evolve` for deterministic testing without a live model (same pattern as `plan`/`run`).

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `Evolve` subcommand: builds provider or `--from` plan, calls `evolve`, honors `--apply` only when the guard authorizes; prints plan path + whether applied + branch. Maps the guard's policy refusal to exit code 4.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(cli): evolve command + e2e (plan-only safety)"`

---

### Task 6: Quality gate + M6 exit

**Step 1:** fmt + clippy + `cargo test --workspace` green.
**Step 2: Verify M6 exit criteria (SPEC-1 §5):** `evolve` produces a self-authoring plan, gated behind `allow_self_modification`, defaulting to plan-only, executing on an isolated branch when applied. Add a test with `allow_self_modification=true` + a clean git fixture asserting the apply lands on `autoagent/evolve/<run-id>` and the original branch is untouched.
**Step 3: Commit** → `git add -A && git commit -m "chore(0.6.0): evolve mode milestone exit"`

---

## Open design questions (resolve during execution)
- Self-test scope: run full `[commands].test`, or only tests touching changed crates? (current: full suite, safest.)
- Whether to auto-merge a successful evolve branch (PROPOSED: never — leave the branch for human review, consistent with FR-27 no-auto-push).
- `git2` dependency vs guarded `git` CLI (current: guarded CLI; revisit if porcelain parsing proves brittle).
