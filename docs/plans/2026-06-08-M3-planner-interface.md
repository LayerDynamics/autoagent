# M3 — 0.3.0 Planner Interface Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `lore:execute` to implement this plan task-by-task.
> **Scope guard:** Do ONLY what is listed here. If you discover adjacent issues, note them as a TODO and continue. Do NOT fix them.

**Goal:** Add `plan "<objective>"` — generate a policy-valid structured plan, optionally via an LLM provider, supporting **both local models and the main cloud providers (Anthropic, OpenAI)**, with source-code egress treated as sensitive (opt-in + redaction + local-model option).
**Architecture:** A `planning::planner` orchestrates: build context (from M2 analysis) → call an `LlmProvider` trait → parse the model's JSON into M1's `Plan` → run M1's `plan_validator`. The model **only proposes plans**; it never executes anything (SPEC-1 §1, FR-22).
**Tech Stack:** Rust 2021; **new deps** `tokio` (rt-multi-thread, macros), `reqwest` (json, rustls-tls), `async-trait`. These are the SPEC-1 §12 "optional later dependencies" — introduced here, the first milestone that needs network I/O.
**Practices:** TDD, typed-interfaces-first, contract-first.
**Required skills:** none.
**Prerequisite:** **M1 complete** (Plan, plan_validator, PolicyEngine, errors) and **M2 complete** (ProjectAnalysis for prompt context).
**Design status:** ⚠️ **PROPOSED DESIGN.** SPEC-1 §13 names "plan command, Markdown/JSON plan writer, LLM provider interface, prompt builder" and FR-22 fixes the policy (plans-only, local+cloud, sensitive egress). Everything below — the `LlmProvider` trait shape, the provider list, the prompt format, the redaction rules — is design to confirm during execution. **OQ-4 (redaction policy) and OQ-5 (which providers ship first) from SPEC-1 §8 must be resolved before enabling any cloud provider.**

**Contracts introduced here (new):** `LlmProvider` trait, `PlanRequest`, `ProviderConfig`, `Redactor`. The `Plan` schema is M1's and is **not** redefined.

---

### Task 1: Plan writers (Markdown + JSON) — no LLM needed

**Files:**
- Create: `crates/autoagent-core/src/planning/plan_writer.rs`
- Modify: `crates/autoagent-core/src/planning/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn writes_paired_json_and_md() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let plan = sample_plan(); // helper building a valid Plan
        let (json_path, md_path) = write_plan(root, "add-cache", &plan).unwrap();
        assert!(json_path.as_str().ends_with(".plan.json"));
        assert!(md_path.as_str().ends_with(".plan.md"));
        let md = std::fs::read_to_string(md_path.as_std_path()).unwrap();
        assert!(md.contains("## Operations"));
        // round-trip: the JSON we wrote must re-read as the same Plan
        let reread = crate::planning::plan_reader::read_plan(&json_path).unwrap();
        assert_eq!(reread.objective, plan.objective);
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `write_plan(root, slug, &Plan)` serializes the Plan to `.agent/plans/<timestamp>-<slug>.plan.json` (pretty) and renders a human `.plan.md` (objective, summary, a `## Operations` table of kind+path+reason, `## Validation`, `## Risks`, `## Rollback`). Timestamp format matches M1's run-id stamp (`YYYYMMDDTHHMMSSZ`). Returns both paths.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(planning): JSON + Markdown plan writers"`

---

### Task 2: `LlmProvider` trait + request/response contracts (typed-first)

**Files:**
- Create: `crates/autoagent-core/src/planning/llm/mod.rs`
- Create: `crates/autoagent-core/src/planning/llm/provider.rs`
- Modify: `crates/autoagent-core/Cargo.toml` (add `tokio`, `reqwest`, `async-trait` under workspace deps)

**Step 1: Write the failing test** (a fake in-memory provider proves the trait + parsing):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    struct FakeProvider(String);
    #[async_trait::async_trait]
    impl LlmProvider for FakeProvider {
        fn name(&self) -> &str { "fake" }
        async fn complete(&self, _req: &PlanRequest) -> crate::error::Result<String> { Ok(self.0.clone()) }
    }
    #[tokio::test]
    async fn provider_returns_parseable_plan_json() {
        let p = FakeProvider(r#"{"objective":"o","summary":"s","files_to_read":[],
          "files_to_create":[],"files_to_modify":[],
          "operations":[{"kind":"Create","path":"crates/x.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"x"}],
          "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#.into());
        let raw = p.complete(&PlanRequest{ objective:"o".into(), context:"ctx".into() }).await.unwrap();
        let plan: crate::planning::plan::Plan = serde_json::from_str(&raw).unwrap();
        assert_eq!(plan.operations.len(), 1);
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation**
```rust
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct PlanRequest { pub objective: String, pub context: String }

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    /// Returns the model's raw text, expected to contain a JSON `Plan`.
    async fn complete(&self, req: &PlanRequest) -> Result<String>;
}
```

**Step 4: Run to verify it passes** → `cargo test -p autoagent-core provider` → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(planning): LlmProvider trait + PlanRequest contract"`

---

### Task 3: Redactor (sensitive-egress control — FR-22 / OQ-4)

**Files:**
- Create: `crates/autoagent-core/src/planning/llm/redactor.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn strips_excluded_paths_and_secret_lines() {
        let r = Redactor::new(vec![".env".into(), "*.pem".into()]);
        assert!(r.is_excluded("config/.env"));
        let cleaned = r.scrub("API_KEY=sk-secret\nfn main(){}");
        assert!(!cleaned.contains("sk-secret"));
        assert!(cleaned.contains("fn main"));
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** (PROPOSED redaction rules — confirm under OQ-4):
- `is_excluded(path)` — true if the path matches any workspace `exclude` glob or the built-in secret globs (`.env*`, `*.pem`, `*.key`, `id_rsa*`).
- `scrub(text)` — redact lines matching secret patterns (`(?i)(api[_-]?key|secret|token|password)\s*[:=]` → replace value with `<redacted>`).
Only files NOT excluded are ever placed in a `PlanRequest.context`, and their content is `scrub`bed first. This is the enforcement point for "no code egress without opt-in" — the planner refuses to call a cloud provider unless `ProviderConfig.code_egress_opt_in == true`.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(planning): redactor for sensitive-egress control"`

---

### Task 4: Local provider (Ollama-style HTTP) — default, no egress

**Files:**
- Create: `crates/autoagent-core/src/planning/llm/local.rs`

**Step 1: Write the failing test** — a wiremock-free test using a oneshot local hyper/`tiny_http` server, or gate behind `#[ignore]` requiring a running local model. PROPOSED: use a minimal in-test HTTP stub.
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test] async fn posts_to_configured_endpoint() {
        // Spin a tiny local server returning a canned completion, point LocalProvider at it,
        // assert complete() returns the body's text field.
        // (implementation in step 3)
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `LocalProvider { endpoint: String, model: String }` implementing `LlmProvider`: POST `{model, prompt}` to `<endpoint>/api/generate` (Ollama contract), parse `{response: "..."}`. This is the **default provider** because it keeps all source on-machine (FR-22 local-model option). Uses `reqwest::Client` with a timeout.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(planning): local (Ollama-style) LLM provider"`

---

### Task 5: Cloud providers (Anthropic, OpenAI) — opt-in egress

**Files:**
- Create: `crates/autoagent-core/src/planning/llm/anthropic.rs`
- Create: `crates/autoagent-core/src/planning/llm/openai.rs`
- Create: `crates/autoagent-core/src/planning/llm/config.rs` (`ProviderConfig`)

**Step 1: Write the failing tests** — request-construction tests (assert headers/body shape against a local stub; do NOT hit real APIs in CI):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn anthropic_request_has_api_version_and_key_header() {
        let req = build_anthropic_request("claude-opus-4-8", "sk-test", "hi");
        assert_eq!(req.headers().get("anthropic-version").unwrap(), "2023-06-01");
        assert!(req.headers().contains_key("x-api-key"));
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `AnthropicProvider`/`OpenAiProvider` implementing `LlmProvider`, reading API keys from **environment** (never from `Autoagent.toml`, per SPEC-1 §3.7 secrets rule). Each refuses to run unless `ProviderConfig.code_egress_opt_in`. `ProviderConfig { kind: Local|Anthropic|OpenAI, model, endpoint?, code_egress_opt_in: bool }` parsed from an optional `[llm]` block added to the config schema (extend M1's `AutoAgentConfig` with `llm: Option<LlmConfig>` — additive, backward compatible). Endpoints: Anthropic `POST https://api.anthropic.com/v1/messages` (header `anthropic-version: 2023-06-01`, `x-api-key`), OpenAI `POST https://api.openai.com/v1/chat/completions` (`Authorization: Bearer`). Emit an `llm_request` event (SPEC-1 §3.4.3) recording `provider`, `model`, `bytes_sent`, `redacted=true`.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(planning): Anthropic + OpenAI providers (env keys, opt-in egress)"`

---

### Task 6: Prompt builder + planner orchestration

**Files:**
- Create: `crates/autoagent-core/src/planning/prompt_builder.rs`
- Create: `crates/autoagent-core/src/planning/planner.rs`
- Modify: `crates/autoagent-core/src/planning/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test] async fn planner_validates_provider_output() {
        let provider = fake_provider_returning_valid_plan(); // returns a Plan that writes under crates/
        let cfg = crate::config::config_schema::AutoAgentConfig::from_toml_str(
            crate::config::default_config::default_toml().as_str()).unwrap();
        let plan = generate_plan("add cache", &cfg, "/ws".into(), &provider).await.unwrap();
        assert_eq!(plan.objective.is_empty(), false);
        // a provider returning a blocked-path op must surface a policy error, not a Plan:
        let bad = fake_provider_returning_git_write();
        assert!(generate_plan("x", &cfg, "/ws".into(), &bad).await.is_err());
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `prompt_builder::build(objective, &ProjectAnalysis, redactor)` produces a prompt instructing the model to emit ONLY a JSON `Plan` matching SPEC-1 §3.4.1 (schema embedded in the prompt), including the policy constraints so the model self-limits. `planner::generate_plan(objective, &config, root, &provider)`: build context (M2 analysis, redacted) → `provider.complete` → extract JSON (first `{`…matching `}`) → `serde_json::from_str::<Plan>` → run M1 `plan_validator::validate_plan` against a `PolicyEngine` from config. A model proposing an illegal op fails validation and returns the policy error — the model never gains write authority.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(planning): prompt builder + planner with mandatory post-validation"`

---

### Task 7: `plan` CLI command + E2E (local provider)

**Files:**
- Create: `crates/autoagent-cli/src/commands/plan.rs`
- Modify: `crates/autoagent-cli/src/main.rs` (`Plan { objective, #[arg(long)] from: Option<PathBuf> }`)
- Create: `crates/autoagent-cli/tests/e2e_plan.rs`

**Step 1: Write the failing E2E** — `plan --from <file>` imports an existing JSON plan (no network), validates it, and writes the paired artifacts. (The LLM path is covered by unit tests with a fake provider; E2E avoids a live model.)
```rust
use std::process::Command;
fn bin() -> &'static str { env!("CARGO_BIN_EXE_autoagent") }
#[test] fn plan_import_writes_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new(bin()).args(["--yes","init"]).current_dir(root).output().unwrap();
    std::fs::write(root.join("in.plan.json"), r#"{"objective":"o","summary":"s","files_to_read":[],
      "files_to_create":[],"files_to_modify":[],"operations":[{"kind":"Create","path":"crates/x.rs",
      "destination_path":null,"reason":"r","before_hash":null,"after_hash":null,"content":"x"}],
      "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#).unwrap();
    let out = Command::new(bin()).args(["plan","--from","in.plan.json","imported objective"])
        .current_dir(root).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let plans: Vec<_> = std::fs::read_dir(root.join(".agent/plans")).unwrap().collect();
    assert!(!plans.is_empty());
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `Plan` subcommand: if `--from`, read+validate+rewrite paired artifacts; else build provider from `[llm]` config (default Local), call `generate_plan` on a tokio runtime, write artifacts. Prints the JSON plan path.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(cli): plan command (import + generate) + e2e"`

---

### Task 8: Quality gate + M3 exit

**Step 1:** fmt + clippy + `cargo test --workspace` green.
**Step 2: Verify M3 exit criteria (SPEC-1 §5):** `plan "<objective>"` emits a policy-valid plan; provider interface works against ≥1 local and ≥1 cloud provider (cloud verified by an `#[ignore]`d live test documented in the test file header); **no code egress without opt-in** (Redactor + `code_egress_opt_in` gate, Task 3/5).
**Step 3: Commit** → `git add -A && git commit -m "chore(0.3.0): planner interface milestone exit"`

---

## Open design questions (resolve during execution — gate cloud enablement)
- **OQ-4:** exact redaction policy (which patterns, secret-detection precision) — Task 3 is a first cut; confirm before shipping cloud providers.
- **OQ-5:** which local runtime (Ollama vs llama.cpp server) and which cloud models ship first.
- Whether to retry/repair malformed model JSON (one re-ask) or fail fast — current: fail fast, repair belongs to M4.
