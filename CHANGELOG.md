# Changelog

All notable changes to AutoAgent are documented here. Versions follow the
SPEC-1 development roadmap; the on-disk plan/run/event schema is frozen at
`schema_version = 1` as of 1.0.0.

## [Unreleased]

### Added

- **More local-first LLM providers.** `lmstudio` and `huggingface-local` speak the
  OpenAI-compatible chat API — on-machine, no code egress — with the full agentic
  tool-calling loop; `huggingface` reaches the hosted Inference API (egress-gated,
  `HF_TOKEN` from the environment). Ollama (`local`) stays the default. A shared
  `OpenAiCompat` core backs the OpenAI-compatible providers.
- **Reproducible session replay across every surface.** `autoagent run --replay
  <id>`, the native `replay` binding, and the `replay()` / `AutoAgent.replay()`
  wrappers in the Python, Node, and Deno SDKs deterministically re-apply a recorded
  session through the policy-gated apply path — no model, no nondeterminism.
- The language SDK functional API + client classes are generated from the bingen
  surface registry, so a new bound symbol ripples into all three SDKs and is
  drift-guarded.

### Changed

- The Python SDK is published to PyPI as **`autoagent-sdk`** (the `autoagent` name
  is taken by an unrelated project); the import name is unchanged —
  `pip install autoagent-sdk` then `from autoagent import AutoAgent`.
- Distribution version reset to **0.0.1** as the initial public-package baseline
  (the `schema_version = 1` on-disk contract is unaffected).

### Fixed

- **Cross-platform support (Windows).** `std::fs::canonicalize` returns `\\?\`
  verbatim paths on Windows, where `/` is a literal character; the path guard now
  de-verbatims them, so the policy escape check, file writes, snapshots, revert,
  and replay behave identically on Windows, macOS, and Linux. CI runs green on all
  three. The agentic `grep` tool emits `/`-separated paths cross-platform.

## [1.0.0] — Stable Release

- Frozen contracts: `Autoagent.toml`, the JSON `Plan` schema (`schemas/plan.schema.json`),
  `run.json`, the `events.jsonl` catalog, and `AutoAgentError` exit codes.
- Schema versioning with a freeze snapshot test; newer-schema artifacts are refused.
- Verified 100% reversibility across every `FileOperationKind`, zero out-of-policy
  writes, and complete monotonic audit trails.
- Performance smoke gate for the scanner; documentation and CI.

## [0.7.0] — Plugin System

- `autoagent-plugin-sdk` with `Plugin`/`Tool`/`HostContext` traits.
- `CoreHost` routes all plugin I/O through the policy engine (no bypass).
- Tool registry with manifest + api-version checks; sandboxed wasmtime WASM host.
- `tools list` command.

## [0.6.0] — Evolve Mode

- Controlled self-authoring: `evolve`, plan-only by default, gated by
  `allow_self_modification`, isolated on `autoagent/evolve/<run-id>` branches.
- Read-only git client; branch-before-evolve.

## [0.5.0] — Memory

- Project / decision / command memory persisted under `.agent/memory`.
- Memory informs subsequent plans and records a decision per completed run.
- `memory` command (show / rebuild / add / remove).

## [0.4.0] — Validation Loop

- `run` supervised workflow: plan → apply → validate → bounded repair → report.
- Guarded command runner; validation report; a run is never `Completed` while failing.

## [0.3.0] — Planner Interface

- LLM provider interface: local (Ollama-style) and cloud (Anthropic, OpenAI).
- Opt-in code egress with a redactor; mandatory post-validation of model plans.
- `plan` command (generate or `--from` import); JSON + Markdown plan writers.

## [0.2.0] — Project Analyzer

- Language / package-manager detection, dependency parsing, source-tree summary.
- `analyze` command writing `project-analysis.md`.

## [0.1.0] — Mutation Engine

- Cargo workspace; error taxonomy; domain types; `Autoagent.toml` schema.
- Path + command guards (proptest-fuzzed); policy engine; file scanner; snapshots.
- Plan reader/validator; event log; file editor + patches; run logger.
- Apply loop; approval gate; revert with drift detection; `init` + `doctor`; CLI.
