# Changelog

All notable changes to AutoAgent are documented here. Versions follow the
SPEC-1 development roadmap; the on-disk plan/run/event schema is frozen at
`schema_version = 1` as of 1.0.0.

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
