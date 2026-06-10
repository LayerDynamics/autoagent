# Agentic read-edit-observe loop + bounded autonomy — implementation plan

**Date:** 2026-06-10
**Status:** proposed (execute in committed increments)

## Goal

Raise AutoAgent's ceiling on non-trivial, multi-file changes by replacing the
single-shot `generate_plan` with an **agentic loop**: the model can call tools
(`read_file`, `grep`, `list_dir`, `run_command`), observe the results, and
iterate before proposing the plan it will apply. Then add an **opt-in autonomous
execution mode** that drives multiple `plan → apply → validate → repair` cycles
to completion for the objective the user gave — without weakening any safety
gate.

## Non-goals / hard safety lines (do NOT cross)

These are load-bearing invariants (SPEC-1 FR-22/FR-23, risk R-5) and are
asserted by `crates/autoagent-cli/tests/doc_sync.rs` ("controlled self-authoring,
not uncontrolled self-replication"):

- The agent NEVER invents its own objectives. It pursues the user's objective.
- Every write/command STILL passes the `PolicyEngine` (path + command policy).
- Every applied change STILL snapshots → patch → is reversible.
- Approval defaults unchanged: `require_approval_before_write`,
  `require_approval_before_command`, `allow_self_modification=false` still hold.
  "Autonomous" means no per-step human prompt *when the user has opted in*
  (config flag / `--yes`), not "no gates."
- No self-replication, no covert persistence, no escaping the workspace.
- The loop is ALWAYS step-bounded (`max_steps_per_run`) and reversible.

## Architecture

### Phase A — tool-use provider surface
- `planning/llm/provider.rs`: add an agentic method alongside `complete`:
  `async fn converse(&self, msgs: &[Message], tools: &[ToolSpec]) -> Result<Turn>`
  where `Turn` is either `ToolCalls(Vec<ToolCall>)` or `Final(String)`.
  Keep `complete` for the one-shot path (back-compat).
- Local (Ollama) provider: implement via `/api/chat` with `tools` (Ollama
  supports tool-calling for capable models) OR, for `/api/generate`-only models,
  a JSON-protocol fallback (the model emits `{"tool":...,"args":...}` or
  `{"final":...}`, constrained by `format`). Anthropic/OpenAI: native tool-use.

### Phase B — read-only context tools (no new write authority)
- New module `runtime/agent_tools.rs`: `read_file`, `grep`, `list_dir` — all
  workspace-confined and redactor-filtered (reuse `Redactor`), plus
  `run_command` routed through `PolicyEngine::check_command` (read-only/allowed
  commands only). These give the model real navigation instead of guessing.

### Phase C — the planning loop
- `planning/agent_planner.rs`: drive the converse loop — system prompt = the
  existing `prompt_builder` schema/role + the tool list; on each `ToolCalls`,
  execute the (read-only, gated) tools and feed results back; stop at `Final`
  (a `Plan` JSON, schema-constrained as in the constrained-JSON work) or at the
  step budget. Falls back to one-shot `generate_plan` when the provider has no
  tool support.

### Phase D — bounded autonomous execution mode
- Config: `[agent] autonomous = false` (default). When true AND approvals are
  satisfied (`--yes` or the approval flags off), `run_workflow` keeps iterating
  plan→apply→validate→repair toward the SAME objective until validation passes
  or `max_steps_per_run` is exhausted — no per-cycle prompt. This is the
  iterative repair loop already built, generalized + opt-in. It still cannot
  invent goals or bypass the policy/snapshot/approval gates.

## Tasks (each its own commit, TDD)

- **AL-1** `Message`/`ToolSpec`/`ToolCall`/`Turn` types + `converse` on the
  provider trait (default impl maps to `complete` for back-compat). Tests.
- **AL-2** `agent_tools.rs`: `read_file`/`grep`/`list_dir`/`run_command`, all
  workspace-confined + redactor/policy-gated. Tests (escape attempts rejected).
- **AL-3** Local provider `converse` over `/api/chat` + JSON-protocol fallback.
  Live smoke test against Ollama.
- **AL-4** `agent_planner` converse loop, schema-constrained final plan, budget
  bound, fallback to one-shot. Tests with a scripted tool-calling provider.
- **AL-5** Wire `run`/`evolve` to prefer the agentic planner when the provider
  supports tools; one-shot otherwise. E2E.
- **AL-6** `[agent] autonomous` opt-in flag → generalized bounded autonomous
  execution in `run_workflow`. Tests: respects budget, gates, reversibility;
  refuses to run mutating ops when approvals/`allow_self_modification` say no.
- **AL-7** Docs: README "agentic loop" + the autonomy bright lines; SPEC update.

## Exit criteria

`cargo test --workspace` green; clippy `-D warnings` + fmt + doc-sync + bingen
drift clean; a live agentic run reads real files before editing; autonomous mode
finishes a multi-step objective unattended while every action remains
policy-gated and reversible; all safety invariants and their tests intact.
