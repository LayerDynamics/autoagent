# SPEC-1: AutoAgent

> A Rust-native local agent runtime for safe, reversible, policy-controlled codebase evolution.

**Date:** 2026-06-08
**Author:** Ryan O'Boyle + Claude
**Status:** Draft
**Version:** 1.0
**Repository:** <https://github.com/LayerDynamics/autoagent>
**Source:** Derived from `AutoAgent Technical Specification.md` (Working Draft, Spec v0.1.0)

---

## 1. Background

### 1.1 Problem Statement

Developers increasingly want an agent that can *change* a codebase — not just suggest edits — but handing mutation authority to an LLM-driven loop is dangerous: it can run arbitrary commands, write outside intended boundaries, touch secrets or Git internals, and leave no reliable way to undo what it did. The unsolved problem is **trust**: how to let an agent inspect, plan, modify, validate, and evolve real source code while guaranteeing that every change is bounded, reviewable, and reversible.

AutoAgent's defining stance is **controlled self-authoring, not uncontrolled self-replication.** The product's marquee capability is that it can eventually work on *its own* source tree — but only as a supervised workflow gated by policies, snapshots, patches, validation, and rollback. It is explicitly *not* a process that persists secretly, spreads across machines, or propagates without supervision. This identity is load-bearing and shapes every requirement below.

### 1.2 Current State

Today a developer choosing to automate code changes faces a gap on both ends:

- **General coding agents / IDE assistants** apply edits through model-driven free-form mutation. They lack a structured policy engine, deterministic snapshot-before-write, a per-run audit trail, and a first-class `revert`. Safety is advisory, not enforced.
- **Scripts and ad-hoc automation** (codemods, shell pipelines) are reversible only if the developer remembered to commit first, and they have no concept of an approval gate or a command allow/block policy.

Neither treats file mutation and command execution as *privileged operations that must pass a policy engine*. AutoAgent makes that the architectural center: the safe mutation engine comes first, and planning intelligence is layered on top only after the engine can already read structured plans, apply changes, validate, and revert.

### 1.3 Target Users

Primary user: **developers who want a controlled local agent to analyze, modify, validate, and evolve codebases** on their own machine, under their own rules. They are comfortable with a CLI, a Rust/Cargo toolchain, and TOML configuration. They value reversibility and an audit trail over hands-off autonomy.

Secondary user (future): the AutoAgent project itself, as the subject of controlled self-authoring (the flagship use case).

### 1.4 Motivation

- **Safety-first opportunity:** existing agents optimize for capability before control. There is room for a tool whose differentiator is that *every* mutation is structured, policy-validated, snapshotted, logged, and reversible.
- **Rust-native fit:** the domain (run states, operations, plans, reports, policies, errors) maps cleanly onto Rust's type system; a single static binary is easy to distribute and trust.
- **Self-authoring as a north star:** a codebase-evolution engine that can safely operate on itself is both a compelling demonstration and a forcing function for getting the safety model right.

### 1.5 Assumptions

- The tool runs **locally**, operating on a workspace the user already controls. It is not a hosted service.
- The user has Rust, Cargo, and Git available (verified by `doctor`).
- The configuration file `Autoagent.toml` is the single source of truth for policy; absence of config means the tool refuses privileged operations rather than guessing.
- The MVP (0.1.0) operates on **user-supplied structured JSON plans**; LLM-generated planning arrives at 0.3.0 and never gains direct unsafe mutation authority — it produces plans, which remain contracts the engine validates.
- "Reversible" assumes the workspace is not concurrently mutated by another process during a run; AutoAgent guarantees reversibility of *its own* applied operations via snapshots and patches.

---

## 2. Requirements

### 2.1 Functional Requirements

| ID | Priority | Requirement |
|----|----------|-------------|
| FR-1 | MUST | The system MUST ship as a single Rust CLI binary exposing the commands: `init`, `doctor`, `analyze`, `plan`, `apply`, `run`, `evolve`, `patch`, `revert`, `memory`, `config`. |
| FR-2 | MUST | `init` MUST create `Autoagent.toml` and the `.agent/` workspace folder tree (`memory/`, `plans/`, `runs/`, `patches/`, `logs/`, `reports/`, `tools/`) after confirmation, or immediately with `--yes`. |
| FR-3 | MUST | `doctor` MUST check Rust, Cargo, Git, config validity, command availability, write permissions, and workspace health, and MUST be strictly read-only. |
| FR-4 | MUST | The system MUST load and validate `Autoagent.toml`; if config is missing or invalid, it MUST refuse privileged (write/command) operations rather than proceed with defaults. |
| FR-5 | MUST | The file scanner MUST enumerate workspace files honoring `include`/`exclude` globs and standard ignore rules (e.g. `.gitignore` semantics), and MUST never traverse blocked paths. |
| FR-6 | MUST | `apply` MUST accept a structured JSON plan and execute its `FileOperation`s only after each operation passes the policy engine (path guard + command guard). |
| FR-7 | MUST | Before mutating any file, the system MUST write a `before/` snapshot of that file into the run folder. |
| FR-8 | MUST | The system MUST support these file operation kinds: Create, Write, Replace, Append, Delete, Rename, CreateDirectory — each recorded with `before_hash`/`after_hash` where applicable. |
| FR-9 | MUST | Every run MUST produce a run folder under `.agent/runs/<run-id>/` containing `run.json`, `objective.md`, `plan.md`, `events.jsonl`, `file-operations.json`, `validation-report.md`, `summary.md`, and `before/`+`after/` snapshot directories. |
| FR-10 | MUST | The system MUST emit an append-only chronological `events.jsonl` event log for each run and a workspace-level `.agent/logs/events.jsonl`. |
| FR-11 | MUST | The system MUST run validation commands (test, lint, format, build per `[commands]`) and produce a `ValidationReport` capturing command, exit code, stdout, stderr, and duration. |
| FR-12 | MUST | `revert <run-id>` MUST restore affected files from the run's `before/` snapshots (or by reversing the stored patch), returning the workspace to its pre-run state for AutoAgent-applied operations. |
| FR-13 | MUST | The command guard MUST execute only commands present in `allowed_commands` (or explicitly approved at runtime) and MUST block any command matching `blocked_commands` or risky fragments (e.g. `sudo`, `rm -rf /`, `curl`, `ssh`). |
| FR-14 | MUST | The path guard MUST normalize and resolve paths, reject workspace escapes and parent traversal, reject `blocked_write_paths` even when a broad allow rule would match, and never write to `.git/`, `target/`, env files, SSH material, or absolute system paths by default. |
| FR-15 | MUST | When `[agent].require_approval_before_write` / `require_approval_before_command` is true (default), the system MUST pause for explicit user approval before the corresponding privileged action. |
| FR-16 | MUST | `patch list` and `patch show <run-id>` MUST list and display stored patch artifacts; saving a patch is the only write `patch` performs. |
| FR-17 | MUST | `config show` MUST display the effective configuration and validate the TOML schema. |
| FR-18 | SHOULD | `analyze` SHOULD detect language, package manager (Cargo / `package.json`), dependencies, and a source-tree summary, and write `reports/project-analysis.md`. |
| FR-19 | SHOULD | `plan` SHOULD create or import a structured plan and write paired `<timestamp>-<slug>.plan.json` and `.plan.md` artifacts. |
| FR-20 | SHOULD | `run` SHOULD chain plan → apply → validate → (optional) repair → report as one supervised workflow, supervised by default. |
| FR-21 | SHOULD | `memory` SHOULD show, rebuild, add, and remove project memory entries (`project.json`, `decisions.json`, `glossary.json`, `commands.json`, `architecture.json`). |
| FR-22 | SHOULD | The system SHOULD expose an LLM provider interface (0.3.0+) that **supports both locally-hosted models and the main cloud providers (e.g. Anthropic, OpenAI)**; the provider produces *plans only* and never receives direct mutation authority. Source code sent to a provider MUST be treated as sensitive (see §3.7). |
| FR-23 | COULD | `evolve` COULD generate a controlled self-authoring plan for AutoAgent's own source, gated behind `allow_self_modification` and **plan-only by default** (no writes without explicit apply). |
| FR-24 | COULD | The system COULD provide a plugin architecture via Rust traits, and eventually WASM plugins, with a tool registry and plugin manifest — all plugins routed through the same safety layer. |
| FR-25 | COULD | `run` COULD perform an autonomous repair pass (inspect validation failures, re-plan, re-apply) within `max_steps_per_run`, still snapshotted and reversible. |
| FR-26 | WONT | The system WILL NOT self-replicate, persist covertly, spread across machines, or modify repositories outside the approved workspace boundary. |
| FR-27 | WONT | The system WILL NOT push to remotes or deploy production changes in this spec's scope (reserved for a future explicit workflow). |
| FR-28 | WONT | The system WILL NOT execute any command, or modify env files / SSH material / Git internals / system paths, without policy validation and (where required) explicit approval. |

### 2.2 Non-Functional Requirements

> **Status of targets:** The latency/throughput numbers below are **proposed defaults derived for a local-first CLI**, not user-confirmed. They are anchored to measurable operations on a defined repository size and are tracked for confirmation in §8 (OQ-1). Safety, reversibility, and audit requirements are firm and derive directly from the source document.

#### Performance

| Metric | Target (proposed) | Measurement |
|--------|-------------------|-------------|
| `analyze` / file scan latency | p95 < 2 s on a 10,000-file repository | Wall-clock of `analyze` against a fixed benchmark repo |
| Per-operation apply overhead | < 100 ms per `FileOperation` excluding the cost of the write itself and validation commands | Run-folder timing in `events.jsonl` |
| Snapshot cost | < 50 ms per touched file (typical source file ≤ 1 MB) | Event timing for snapshot step |
| `revert` latency | p95 < 1 s for a run touching ≤ 50 files | Wall-clock of `revert <run-id>` |
| Startup overhead | < 150 ms cold to first useful output (config load + workspace validation) | `doctor`/`config show` wall-clock |

#### Reliability

| Metric | Target |
|--------|--------|
| Mutation reversibility | 100% of AutoAgent-applied runs are snapshotted and reversible via `revert` |
| Out-of-policy writes | 0 — no write may land outside `allowed_write_paths` / inside `blocked_write_paths` |
| Audit completeness | 100% of file operations and command executions appear in `events.jsonl` with hashes/exit codes |
| Validation integrity | A run is never reported `Completed` if its `ValidationReport.passed` is false (it is `Failed` or `Repairing`) |
| Atomicity of failure | A partially-applied run that fails MUST be fully revertible from its `before/` snapshots |

#### Security & Compliance

- **Auth model:** None required for local operation — AutoAgent runs as the invoking user with that user's filesystem permissions. There is no network-facing surface in the MVP.
- **Authorization model:** Policy-as-config. The `[safety]` block (allow/block path lists, allow/block command lists) plus `[agent]` approval gates are the authorization layer. Self-modification is a distinct, default-off capability (`allow_self_modification = false`).
- **Data sensitivity:** No PII/PHI/financial data is handled by the tool itself. The one sensitive egress is **source code sent to LLM providers** once the planner interface lands (FR-22); this is treated as sensitive data and governed in §3.7 (opt-in, local-model option, redaction).
- **Compliance regime:** None applicable (local developer tool). See §3.7 for the LLM data-handling controls that substitute for a formal regime.
- **Audit logging:** Mandatory and always-on — every run is fully reconstructable from its run folder.

#### Scalability

This is a single-user, single-machine tool; "scale" means **repository size and run history**, not concurrent users.

- Scanning MUST stream/iterate rather than load the whole tree into memory; target working comfortably on repos up to ~100k files via the `ignore`/`walkdir` crates.
- Run history under `.agent/runs/` grows unbounded by design (audit trail); pruning/retention is an explicit user action, never automatic (no silent deletion of audit state).
- Memory store (`.agent/memory/*.json`) is bounded by project size, not run count.

### 2.3 Constraints

- **Language/runtime:** Rust, delivered as a Cargo workspace with separate crates (`autoagent-cli`, `autoagent-core`, `autoagent-plugin-sdk`). Core must be independently testable without the CLI.
- **Config format:** TOML (`Autoagent.toml`) — chosen over executable JS config for safety and Rust-native parsing.
- **Dependency budget (0.1.0):** `anyhow`, `thiserror`, `clap` (derive), `serde`/`serde_json`, `toml`, `walkdir`, `ignore`, `globset`, `similar`, `console`, `dialoguer`, `indicatif`, `chrono`, `uuid`, `sha2`, `camino`. Async/network/Git-lib/tree-sitter deps are deferred to later milestones.
- **Execution model:** Local-first, policy-driven, supervised by default.
- **Architectural invariant:** the safe mutation engine MUST be correct before the planner becomes powerful — planning intelligence never bypasses the engine.

### 2.4 Explicit Non-Goals

- Uncontrolled self-replication, covert persistence, or cross-machine propagation. (FR-26)
- Pushing to remotes or deploying production changes. (FR-27)
- Executing arbitrary commands without policy validation/approval. (FR-28)
- Modifying env files, SSH material, Git internals, or system paths by default. (FR-14)
- Acting as a hosted/multi-user service.
- Making the LLM responsible for direct unsafe mutation — the model plans; the engine applies validated contracts.

---

## 3. Architecture

### 3.1 System Overview

AutoAgent is a Rust workspace split so the user-facing CLI is independent from, and thin relative to, the mutation engine. The `autoagent-cli` crate parses arguments, renders output, and asks for confirmations; `autoagent-core` holds all privileged logic (config, analysis, planning, editing, validation, safety, memory, logging, git, errors); `autoagent-plugin-sdk` defines future extension contracts.

```text
                ┌──────────────────────────────────────────────┐
   user ──────► │              autoagent-cli                   │
  (terminal)    │  clap commands · prompts · report rendering  │
                └───────────────────────┬──────────────────────┘
                                        │ calls (no privileged logic in CLI)
                                        ▼
   ┌───────────────────────────── autoagent-core ─────────────────────────────┐
   │                                                                           │
   │   runtime ──► agent_loop / task_context / run_state                       │
   │      │                                                                    │
   │      ├─► config    (load + validate Autoagent.toml)                       │
   │      ├─► analysis  (project_analyzer, file_scanner, source_map)           │
   │      ├─► planning   (plan, plan_reader/writer, plan_validator)            │
   │      │        │                                                           │
   │      │        ▼                                                           │
   │      ├─► safety  ◄── EVERY privileged op passes here ──┐                  │
   │      │     policy_engine · path_guard · command_guard · approval_gate     │
   │      │        │                                        │                  │
   │      ├─► editing (snapshot_manager → file_editor → diff/patch_writer)     │
   │      ├─► validation (command_runner → validation_report)                  │
   │      ├─► memory  (project / decisions / commands / architecture)          │
   │      ├─► logging (run_logger → events.jsonl)                              │
   │      └─► git     (branch_manager — branch-before-evolve)                  │
   │                                                                           │
   └───────────────────────────────┬───────────────────────────────────────────┘
                                   ▼
                       .agent/  workspace artifacts
        (memory · plans · runs/<id> · patches · logs · reports · tools)
                                   ▲
                                   │ future
                       autoagent-plugin-sdk (traits → WASM)
```

**Load-bearing invariant:** the `safety` module is on the path of every read-that-could-write, every write, and every command. Built-in tools and future plugins are required to route through it; there is no privileged path that bypasses the policy engine.

### 3.2 Component Design

#### Component: autoagent-cli

- **Responsibility:** Translate terminal input into core calls and render core results; own all human interaction (confirmations, progress, reports).
- **Technology:** Rust, `clap` (derive), `console`, `dialoguer`, `indicatif`.
- **Interfaces:** Subcommands `init/doctor/analyze/plan/apply/run/evolve/patch/revert/memory/config`.
- **Dependencies:** `autoagent-core`. Holds **no** policy or mutation logic.

#### Component: Runtime (`runtime/`)

- **Responsibility:** Orchestrate a single run through its state machine (`RunState`) using a `TaskContext`.
- **Technology:** Rust; `uuid` (run id), `chrono` (timestamps).
- **Interfaces:** `agent_runtime` entry point; `agent_loop` driving the 14-step loop (§3.5); `run_state`/`task_context` types.
- **Dependencies:** config, analysis, planning, safety, editing, validation, memory, logging.

#### Component: Config (`config/`)

- **Responsibility:** Load, validate, and supply `Autoagent.toml` as a typed `AutoAgentConfig`; refuse privileged ops when invalid/absent.
- **Technology:** `serde`, `toml`.
- **Interfaces:** `config_loader`, `config_schema`, `default_config`.
- **Dependencies:** error module.

#### Component: Analysis (`analysis/`)

- **Responsibility:** Scan the workspace and summarize structure, language, dependencies, and a source map — read-only.
- **Technology:** `walkdir`, `ignore`, `globset`.
- **Interfaces:** `project_analyzer`, `file_scanner`, `dependency_analyzer`, `source_map_builder`.
- **Dependencies:** config (for include/exclude), safety (path guard for traversal limits).

#### Component: Planning (`planning/`)

- **Responsibility:** Read/write structured plans and validate that every plan operation is policy-legal *before* execution.
- **Technology:** `serde_json` (JSON plans), `similar` (diff context), Markdown writer.
- **Interfaces:** `planner`, `plan`, `plan_reader`, `plan_writer`, `plan_validator`.
- **Dependencies:** editing (`FileOperation`), safety.

#### Component: Editing (`editing/`)

- **Responsibility:** Snapshot-before-write, apply file operations, compute diffs, and persist patches.
- **Technology:** `sha2` (content hashes), `similar` (diffs), `camino` (UTF-8 paths).
- **Interfaces:** `snapshot_manager`, `file_editor`, `file_operation`, `diff_builder`, `patch_writer`.
- **Dependencies:** safety (each op validated), logging.

#### Component: Validation (`validation/`)

- **Responsibility:** Execute policy-approved validation commands and capture structured results.
- **Technology:** `std::process` via a guarded `command_runner`.
- **Interfaces:** `command_runner`, `validation_report`.
- **Dependencies:** safety (command guard), logging.

#### Component: Safety (`safety/`)

- **Responsibility:** The single chokepoint that authorizes (or denies) every path and command, and enforces approval gates.
- **Technology:** `globset`, path normalization, `dialoguer` (via CLI) for approvals.
- **Interfaces:** `policy_engine`, `path_guard`, `command_guard`, `approval_gate`.
- **Dependencies:** config (policy lists), logging.

#### Component: Memory (`memory/`)

- **Responsibility:** Persist and serve durable project knowledge across runs.
- **Technology:** `serde_json`.
- **Interfaces:** `memory_store`, `project_memory`, `decision_log`.
- **Dependencies:** config.

#### Component: Logging (`logging/`)

- **Responsibility:** Produce the append-only event stream and per-run logs that make every run auditable.
- **Technology:** `serde_json` (JSONL), `chrono`.
- **Interfaces:** `logger`, `run_logger`, `event_log`.
- **Dependencies:** none beyond error.

#### Component: Git (`git/`)

- **Responsibility:** Read-only Git status/diff inspection and **branch-before-evolve** for self-authoring; never pushes.
- **Technology:** policy-approved `git` CLI invocations (optionally `git2` later).
- **Interfaces:** `git_client`, `branch_manager`.
- **Dependencies:** safety (command guard).

#### Component: Plugin SDK (`autoagent-plugin-sdk`) — *future*

- **Responsibility:** Define plugin/tool/manifest contracts and the eventual WASM extension surface.
- **Technology:** Rust traits, `schema`; later WASM runtime.
- **Interfaces:** `plugin`, `tool`, `schema`.
- **Dependencies:** must consume the same safety layer; no privileged bypass.

### 3.3 Data Model

Core domain types (verbatim from the source spec, §8), serialized with `serde`:

**RunState** — the run state machine:
`Created → LoadingConfig → AnalyzingProject → LoadingMemory → Planning → AwaitingApproval → Snapshotting → ApplyingChanges → Validating → (Repairing) → Completed | Failed | Reverted`

**FileOperation** — the unit of mutation:

```rust
pub enum FileOperationKind { Create, Write, Replace, Append, Delete, Rename, CreateDirectory }

pub struct FileOperation {
    pub kind: FileOperationKind,
    pub path: Utf8PathBuf,
    pub destination_path: Option<Utf8PathBuf>, // for Rename
    pub reason: String,
    pub before_hash: Option<String>,           // sha2 of prior content
    pub after_hash: Option<String>,            // sha2 of new content
    pub content: Option<String>,
}
```

**TaskContext** — per-run identity and policy snapshot:

```rust
pub struct TaskContext {
    pub id: Uuid,
    pub run_id: String,
    pub objective: String,
    pub root_directory: Utf8PathBuf,
    pub mode: AgentMode,                // PlanOnly | Supervised | Apply | Autonomous | Evolve
    pub self_modification: bool,
    pub state: RunState,
    pub config: AutoAgentConfig,
    pub created_at: DateTime<Utc>,
}
```

**Plan** — the contract the engine applies:

```rust
pub struct Plan {
    pub objective: String,
    pub summary: String,
    pub files_to_read: Vec<Utf8PathBuf>,
    pub files_to_create: Vec<PlannedFile>,   // { path, purpose }
    pub files_to_modify: Vec<PlannedFile>,
    pub operations: Vec<FileOperation>,
    pub validation_commands: Vec<String>,
    pub risks: Vec<String>,
    pub rollback_strategy: String,
}
```

**ValidationReport** — structured command outcomes:

```rust
pub struct ValidationReport { pub passed: bool, pub commands: Vec<CommandValidationResult> }
pub struct CommandValidationResult {
    pub command: String, pub exit_code: Option<i32>,
    pub stdout: String, pub stderr: String, pub duration_ms: u128,
}
```

**Persistent entities (on disk, not in-memory types):**

| Entity | Location | Lifecycle |
|--------|----------|-----------|
| Config | `Autoagent.toml` | Created by `init`; user-edited; read every run |
| Memory | `.agent/memory/{project,decisions,glossary,commands,architecture}.json` | Created/updated by `memory` and run completion; long-lived |
| Plan | `.agent/plans/<timestamp>-<slug>.plan.{json,md}` | Created by `plan`/`run`; immutable once written |
| Run | `.agent/runs/<run-id>/…` | Created per run; **append-only, never auto-deleted** (audit trail) |
| Patch | `.agent/patches/<run-id>.patch` | Created before/at write; consumed by `revert` |
| Reports | `.agent/reports/project-analysis.md` | Overwritten by `analyze` |

Consistency: strong/local — all state is files on the user's disk written by a single process per run. No distributed consistency concerns.

### 3.4 API & Interface Design

The "API" is the CLI surface plus the on-disk JSON/Markdown contracts.

**CLI (canonical invocations):**

```bash
autoagent init
autoagent doctor
autoagent analyze
autoagent plan "add plugin support"
autoagent apply .agent/plans/add-plugin-support.plan.json
autoagent run "fix failing tests"
autoagent evolve "improve the planner"          # plan-only by default
autoagent patch list | patch show <run-id>
autoagent revert <run-id>
autoagent memory show
autoagent config show
```

| Command | Purpose | Default write behavior |
|---------|---------|------------------------|
| `init` | Create `Autoagent.toml` + `.agent/` | Writes after confirmation / `--yes` |
| `doctor` | Health-check toolchain, config, perms | Read-only |
| `analyze` | Scan + write project report | Writes report only |
| `plan` | Create/import structured plan | Writes plan files only |
| `apply` | Apply plan via snapshots + policy + validation | Writes only approved planned changes |
| `run` | Plan→apply→validate→(repair)→report | Supervised by default |
| `evolve` | Self-authoring on AutoAgent itself | Plan-only by default |
| `patch` | List/show/save patches | Read-only unless saving |
| `revert` | Restore from snapshots / reverse patch | Writes rollback changes |
| `memory` | Show/rebuild/add/remove memory | Depends on subcommand |
| `config` | Show/validate config | Read-only |

#### 3.4.1 JSON Plan Schema (`*.plan.json`)

The plan is the contract `apply` consumes. It is the `Plan` struct (§3.3) serialized with `serde_json`. Field-level contract:

| Field | JSON type | Required | Constraint enforced by `plan_validator` |
|-------|-----------|----------|------------------------------------------|
| `objective` | string | yes | non-empty, ≤ 2,000 chars |
| `summary` | string | yes | non-empty |
| `files_to_read` | string[] | yes (may be `[]`) | each path workspace-relative; each must pass path guard *read* check |
| `files_to_create` | object[] `{path, purpose}` | yes (may be `[]`) | `path` must not already exist; `path` must pass path guard *write* check; `purpose` non-empty |
| `files_to_modify` | object[] `{path, purpose}` | yes (may be `[]`) | `path` must exist; must pass path guard *write* check |
| `operations` | object[] `FileOperation` | yes, ≥ 1 | every op validated (table below) |
| `validation_commands` | string[] | yes (may be `[]`) | each command must match `[safety].allowed_commands` exactly, or be flagged for runtime approval |
| `risks` | string[] | yes (may be `[]`) | informational; surfaced in `summary.md` |
| `rollback_strategy` | string | yes | non-empty; for MVP must be `"snapshot"` (the only supported strategy in 0.1.0) |

Per-`operations[]` element validation:

| `kind` | Required fields | `plan_validator` rule |
|--------|-----------------|------------------------|
| `Create` | `path`, `content`, `reason` | `path` must not exist; write-allowed; `content` present |
| `Write` / `Replace` | `path`, `content`, `reason` | `path` must exist; write-allowed; `before_hash` recomputed at apply time |
| `Append` | `path`, `content`, `reason` | `path` must exist; write-allowed |
| `Delete` | `path`, `reason` | `path` must exist; write-allowed; never a blocked path |
| `Rename` | `path`, `destination_path`, `reason` | both paths write-allowed; source exists, destination does not |
| `CreateDirectory` | `path`, `reason` | parent within workspace; write-allowed |

A plan failing **any** rule is rejected wholesale (no partial apply); the failing rule is written to `events.jsonl` as a `plan_rejected` event with the offending field/op index. Example minimal plan:

```json
{
  "objective": "Add a license header to lib.rs",
  "summary": "Insert SPDX header at top of crates/autoagent-core/src/lib.rs",
  "files_to_read": ["crates/autoagent-core/src/lib.rs"],
  "files_to_create": [],
  "files_to_modify": [{ "path": "crates/autoagent-core/src/lib.rs", "purpose": "prepend header" }],
  "operations": [
    {
      "kind": "Replace",
      "path": "crates/autoagent-core/src/lib.rs",
      "destination_path": null,
      "reason": "prepend SPDX header",
      "before_hash": null,
      "after_hash": null,
      "content": "// SPDX-License-Identifier: MIT\n\n<existing file body>"
    }
  ],
  "validation_commands": ["cargo build", "cargo fmt --all -- --check"],
  "risks": ["none material"],
  "rollback_strategy": "snapshot"
}
```

`before_hash`/`after_hash` are written as `null` in an authored plan and **populated by the engine** at apply time (sha256, lowercase hex). On revert, the stored `before_hash` is the integrity check.

#### 3.4.2 `run.json` Schema

Machine-readable record written once at run completion (and updated to `Reverted` by `revert`):

```json
{
  "run_id": "20260608T142233Z-add-license-header",
  "task_id": "550e8400-e29b-41d4-a716-446655440000",
  "objective": "Add a license header to lib.rs",
  "mode": "Supervised",
  "self_modification": false,
  "state": "Completed",
  "started_at": "2026-06-08T14:22:33Z",
  "ended_at": "2026-06-08T14:22:41Z",
  "duration_ms": 8123,
  "plan_path": ".agent/plans/20260608T142233Z-add-license-header.plan.json",
  "files_read": ["crates/autoagent-core/src/lib.rs"],
  "files_modified": [
    { "path": "crates/autoagent-core/src/lib.rs", "kind": "Replace",
      "before_hash": "e3b0c4…", "after_hash": "a94a8f…" }
  ],
  "commands_executed": [
    { "command": "cargo build", "exit_code": 0, "duration_ms": 5400 },
    { "command": "cargo fmt --all -- --check", "exit_code": 0, "duration_ms": 210 }
  ],
  "validation_passed": true,
  "patch_path": ".agent/patches/20260608T142233Z-add-license-header.patch",
  "approvals": [{ "kind": "write", "granted": true, "at": "2026-06-08T14:22:36Z" }],
  "reverted_at": null
}
```

`run_id` format is `<UTC-timestamp:YYYYMMDDTHHMMSSZ>-<slug>`; it is the directory name under `.agent/runs/` and the join key for `patches/<run-id>.patch`.

#### 3.4.3 Event Catalog (`events.jsonl`)

One JSON object per line, append-only, never rewritten. Common envelope on every event:

```json
{ "ts": "2026-06-08T14:22:36.412Z", "run_id": "20260608T142233Z-add-license-header",
  "seq": 7, "type": "<event-type>", "state": "<RunState>", "data": { … } }
```

`seq` is a monotonic per-run counter (gaps are an integrity error). Event `type` catalog:

| `type` | Emitted when | Key `data` fields |
|--------|--------------|-------------------|
| `run_started` | run id + folder created | `objective`, `mode` |
| `config_loaded` | config parsed + valid | `config_path` |
| `analysis_completed` | scan finished | `files_scanned`, `language` |
| `memory_loaded` | memory read | `entries` |
| `plan_loaded` | plan read/generated | `plan_path`, `operation_count` |
| `plan_rejected` | validation failed | `op_index`, `field`, `rule` |
| `approval_requested` / `approval_granted` / `approval_denied` | gate hit / resolved | `kind` (`write`\|`command`), `target` |
| `snapshot_created` | file copied to `before/` | `path`, `before_hash` |
| `operation_applied` | one `FileOperation` done | `kind`, `path`, `after_hash` |
| `operation_failed` | apply error mid-op | `kind`, `path`, `error_code` |
| `patch_written` | patch persisted | `patch_path` |
| `command_started` / `command_finished` | validation command | `command`, `exit_code`, `duration_ms` |
| `validation_completed` | report built | `passed`, `failed_command` |
| `run_completed` / `run_failed` | terminal state | `state`, `error_code?` |
| `revert_started` / `revert_completed` | during `revert` | `restored_files`, `drift_detected` |
| `drift_detected` | `before_hash` mismatch on revert | `path`, `expected_hash`, `actual_hash` |
| `llm_request` (0.3.0+) | provider called | `provider`, `model`, `bytes_sent`, `redacted` |

This catalog is the audit contract: any run is fully reconstructable by replaying its events in `seq` order.

### 3.5 Data Flow

The agent runtime loop (source §7), traced through components:

1. **Receive** objective or plan file → CLI builds initial `TaskContext` (`Created`).
2. **Load** `Autoagent.toml` → config (`LoadingConfig`); abort if invalid.
3. **Validate** workspace root + policy boundaries → safety/path_guard.
4. **Analyze** project files + metadata → analysis (`AnalyzingProject`).
5. **Load** project memory → memory (`LoadingMemory`).
6. **Create** task context + run id (`uuid`) and run folder.
7. **Generate or read** a structured plan → planning (`Planning`).
8. **Validate** every plan operation against path + command policy → safety + `plan_validator`.
9. **Approve:** if supervised policy requires it, pause for user approval → approval_gate (`AwaitingApproval`).
10. **Snapshot** every touched file into `runs/<id>/before/` → snapshot_manager (`Snapshotting`).
11. **Apply** file operations → file_editor (`ApplyingChanges`); copy results into `after/`.
12. **Record** patch (`patches/<id>.patch`) + events (`events.jsonl`).
13. **Run** validation commands → command_runner (`Validating`); build `ValidationReport`.
14. **Write** `summary.md`, update memory if appropriate; terminal state `Completed` / `Failed`; `revert` later yields `Reverted`.

Revert flow: `revert <run-id>` reads the run folder, and for each applied operation restores the `before/` snapshot (or applies the reverse patch), verifying `before_hash` to detect external drift, then records the run as `Reverted`.

### 3.6 Integration Points

- **Local toolchain (required):** Rust, Cargo, Git — invoked only through the command guard (`cargo test/build/fmt/clippy`, `git status/diff/branch/checkout`).
- **LLM providers (0.3.0+):** a provider interface supporting **both local models (e.g. an Ollama-style local endpoint) and the main cloud providers (Anthropic, OpenAI)**. Providers receive context and return *plans*; they never receive mutation authority. This is the only outbound network integration and is governed by §3.7.
- **Future plugins (0.7.0):** Rust-trait and WASM plugins via `autoagent-plugin-sdk`, routed through the safety layer and tool registry.

### 3.7 Security Architecture

**Policy as the authorization layer.** There is no user authentication (single local user); authorization is entirely the `[safety]` + `[agent]` config evaluated by the policy engine on every privileged op:

**Path guard algorithm** — `path_guard::check(path, access: Read|Write) -> Result<Utf8PathBuf, PolicyError>`. Default-deny: a path is allowed only if it survives every step.

```text
1. REJECT if path is empty or contains a NUL byte.
2. CANONICALIZE intent:
   a. If absolute, keep as-is; if relative, join onto workspace_root.
   b. Lexically normalize "." and ".." segments WITHOUT touching disk
      (so a non-existent create target can still be checked).
3. ESCAPE CHECK: the normalized path MUST be workspace_root itself or a
   descendant of it. Any path that resolves outside → REJECT (PolicyError::PathEscape).
4. SYMLINK RESOLUTION (best-effort): for the longest existing prefix of the
   path, realpath it. If the resolved real path escapes workspace_root → REJECT
   (PolicyError::SymlinkEscape). A symlink whose target is outside is never followed for Write.
5. BLOCK LIST (highest precedence): if the path is under any entry of
   [safety].blocked_write_paths (default: .git/, target/, .env, .env.local,
   .ssh/, "/", "../") OR matches built-in deny globs (.env*, node_modules/) → REJECT
   (PolicyError::BlockedPath). Block ALWAYS overrides allow.
6. For access == Write: the path MUST be under at least one entry of
   [safety].allowed_write_paths. No match → REJECT (PolicyError::NotAllowed).
   For access == Read: the path MUST be inside the workspace and match
   [workspace].include minus [workspace].exclude.
7. ALLOW → return the normalized path.
```

Precedence is fixed: **escape > symlink-escape > block > allow**. A broad allow (`crates/`) can never re-permit a blocked path (`crates/.env`).

**Command guard algorithm** — `command_guard::check(cmd: &str) -> Result<Approved, PolicyError>`:

```text
1. TRIM and collapse internal whitespace to single spaces (canonical form).
2. REJECT if the canonical command contains a blocked SUBSTRING/fragment from
   [safety].blocked_commands or the built-in deny set
   (sudo, "rm -rf /", curl, wget, ssh, scp, "chmod 777", chown) → PolicyError::BlockedCommand.
   Fragment match is substring-based so "X && sudo Y" is caught.
3. REJECT shell metacharacters that enable chaining/redirection unless the whole
   command is an exact allow-list entry: ; | & > < ` $( ) → PolicyError::UnsafeShellSyntax.
   (Prevents an allowed prefix from smuggling a blocked suffix.)
4. ALLOW-EXACT: if canonical command == an entry in [safety].allowed_commands → Approved::Policy.
5. Otherwise it is UNKNOWN: if [agent].require_approval_before_command → prompt
   approval_gate; granted → Approved::User, denied → PolicyError::CommandNotApproved.
6. EXECUTION: run with cwd = workspace_root, inherited env minus secrets, capturing
   stdout/stderr/exit_code/duration into CommandValidationResult; emit command_started/finished events.
```

**Approval gates** — `require_approval_before_write` and `require_approval_before_command` (both default true) force explicit confirmation; any UNKNOWN command requires approval; `evolve` is plan-only unless explicitly applied. Approvals are recorded as events and in `run.json.approvals[]`.

**Self-modification** — `allow_self_modification = false` by default; enabling it still keeps `evolve` plan-only and triggers branch-before-evolve (`branch_manager` creates `autoagent/evolve/<run-id>`) so self-authoring happens on an isolated Git branch, never on the checked-out working branch.

**LLM data-handling controls (the substitute for a formal compliance regime):**

- Sending source code to a provider is **opt-in**; no code leaves the machine unless the user configures and selects a provider.
- A **local-model option** MUST be supported so users can avoid any cloud egress entirely.
- The spec requires a redaction/exclusion hook so secrets and `exclude`d paths are never forwarded to a provider.
- Provider calls are logged as events (which provider, what was sent at a summary level) for auditability — open item OQ-4 covers the exact redaction policy.

**Secrets:** AutoAgent never reads or writes `.env*`/SSH material by default; any provider API keys live in the user's environment, not in `Autoagent.toml`.

### 3.8 Resilience Design

- **Failure handling:** a failed apply leaves a fully-snapshotted run that `revert` restores atomically from `before/`. `run`'s optional repair pass (0.4.0+) re-plans within `max_steps_per_run`, never exceeding the step budget.
- **Drift detection:** `before_hash`/`after_hash` let `revert` detect files changed outside AutoAgent since the run, and warn rather than blindly overwrite.
- **Idempotence:** operations carry enough metadata (kind, hashes, content) to be replayed or reversed deterministically.
- **No backpressure/rate-limiting needed** in the local MVP; when the LLM provider lands, provider calls get retry-with-backoff and a timeout (deferred deps: `tokio`, `reqwest`).
- **Caching:** analysis/source-map results MAY be cached per workspace; cache is advisory and never a substitute for re-validating policy at apply time.

### 3.9 Observability

The audit trail *is* the observability story — derived from event logs and run reports rather than an external metrics stack:

- **Logging:** append-only `events.jsonl` per run and at workspace level (`logging.level` configurable, default `info`); human-readable `summary.md`, `validation-report.md`, `objective.md`, `plan.md` per run.
- **Run metadata:** `run.json` gives machine-readable status, files read/modified, commands executed, and validation results — the queryable record of every run.
- **Metrics/tracing:** no Prometheus/OTel in scope; per-operation durations are captured in events and `CommandValidationResult.duration_ms`. A future metrics surface is noted as OQ-3.
- **Health:** `doctor` is the on-demand health probe (toolchain, config, permissions, command availability).

### 3.10 Infrastructure & Deployment

- **Artifact:** a single statically-linked Rust binary (`autoagent`). No server, no container required to run.
- **Build/CI:** Cargo workspace; CI runs `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo build` (the same commands the tool guards for user projects).
- **Environments:** there is one "environment" — the user's local machine. No staging/prod topology.
- **Distribution:** intended via `cargo install` and/or tagged GitHub release binaries — exact channel is **OQ-2** (open).
- **Config bootstrap:** `init` scaffolds `Autoagent.toml` + `.agent/`; `doctor` verifies readiness.

### 3.11 Error Model

All fallible core operations return `Result<T, AutoAgentError>`. `AutoAgentError` is a `thiserror` enum; the CLI maps each variant to a stable process exit code and a human message, while the originating event records the machine `error_code`. Policy denials are a distinct sub-enum (`PolicyError`) so a *refusal* is never confused with a *crash*.

```rust
// crates/autoagent-core/src/error/autoagent_error.rs
#[derive(Debug, thiserror::Error)]
pub enum AutoAgentError {
    #[error("configuration error: {0}")]      Config(String),        // missing/invalid Autoagent.toml
    #[error("workspace error: {0}")]          Workspace(String),     // root missing, not writable
    #[error("analysis error: {0}")]           Analysis(String),
    #[error("plan error: {0}")]               Plan(String),          // unparseable / schema-invalid plan
    #[error("policy denied: {0}")]            Policy(#[from] PolicyError),
    #[error("editing error: {0}")]            Editing(String),       // snapshot/apply IO failure
    #[error("validation failed: {0}")]        Validation(String),    // a validation command exited non-zero
    #[error("revert error: {0}")]             Revert(String),
    #[error("memory error: {0}")]             Memory(String),
    #[error("io error: {0}")]                 Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]      Serde(String),
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("path escapes workspace: {0}")]        PathEscape(String),
    #[error("symlink target escapes workspace: {0}")] SymlinkEscape(String),
    #[error("path is blocked by policy: {0}")]     BlockedPath(String),
    #[error("path not in allowed write paths: {0}")] NotAllowed(String),
    #[error("command is blocked by policy: {0}")]  BlockedCommand(String),
    #[error("unsafe shell syntax: {0}")]           UnsafeShellSyntax(String),
    #[error("command requires approval and was denied: {0}")] CommandNotApproved(String),
    #[error("write requires approval and was denied: {0}")]   WriteNotApproved(String),
}
```

Exit-code and `error_code` mapping (stable contract for scripting):

| Variant | Exit code | `error_code` | Reversible state left behind |
|---------|-----------|--------------|------------------------------|
| `Config` | 2 | `config` | n/a — fails before any run folder |
| `Workspace` | 2 | `workspace` | n/a |
| `Plan` | 3 | `plan` | run folder + `plan_rejected` event, no writes |
| `Policy(_)` | 4 | `policy.<sub>` (e.g. `policy.blocked_path`) | no write performed; refusal logged |
| `Editing` | 5 | `editing` | partial run is snapshotted → `revert` restores |
| `Validation` | 6 | `validation` | changes applied but `validation_passed=false`; user may `revert` |
| `Revert` | 7 | `revert` | drift surfaced, nothing overwritten |
| `Memory` | 8 | `memory` | run unaffected |
| `Io` / `Serde` | 1 | `io` / `serde` | depends on phase; snapshots preserved |

**Invariant:** a `Policy(_)` error is raised *before* any byte is written or any command runs — refusal is always free of side effects. An `Editing` error mid-apply always leaves a complete `before/` snapshot, so the half-applied run is fully revertible (ties to R-2).

---

## 4. Implementation Plan

### 4.1 Build Phases

Phases follow the source roadmap (§13). Guiding principle: **the mutation engine must be safe before the planner becomes powerful.**

#### Phase 1 (0.1.0): Rust Mutation Engine

- **Goal:** A safe, reversible engine that applies user-supplied JSON plans.
- **Scope:** Workspace + crates scaffold; `Autoagent.toml` loader; `.agent/` init; `doctor`; file scanner; policy engine (path + command guard); snapshot manager; JSON plan reader; `apply`; `revert`; JSONL event logs.
- **Exit criteria:** `init`, `doctor`, `apply <plan.json>`, and `revert <run-id>` work end-to-end on a sample repo; every applied run is snapshotted, logged, and fully reversible; zero out-of-policy writes in the test suite.

#### Phase 2 (0.2.0): Project Analyzer

- **Goal:** Understand a codebase to inform plans.
- **Scope:** Language detection, Cargo/`package.json` detection, dependency summaries, source-tree summaries, `project-analysis.md` writer.
- **Exit criteria:** `analyze` produces an accurate report on Rust and JS/TS sample repos; honors include/exclude and ignore rules.

#### Phase 3 (0.3.0): Planner Interface

- **Goal:** Generate plans, including via LLM — without giving the model mutation authority.
- **Scope:** `plan` command; Markdown + JSON plan writers; LLM provider interface (local + main cloud providers); prompt builder; redaction hook.
- **Exit criteria:** `plan "<objective>"` emits a policy-valid plan; provider interface works against at least one local and one cloud provider; no code egress without opt-in.

#### Phase 4 (0.4.0): Validation Loop

- **Goal:** One-shot supervised plan→apply→validate→repair→report.
- **Scope:** `run` command; validation execution; failure inspection; repair pass (bounded by `max_steps_per_run`); final report.
- **Exit criteria:** `run "<objective>"` completes a supervised workflow; failed validation triggers a bounded repair attempt; result is reversible.

#### Phase 5 (0.5.0): Memory

- **Goal:** Persist project knowledge across runs.
- **Scope:** project/decision/command/architecture memory; `memory` subcommands.
- **Exit criteria:** memory survives across runs and measurably informs subsequent plans/reports.

#### Phase 6 (0.6.0): Evolve Mode

- **Goal:** Controlled self-authoring of AutoAgent itself.
- **Scope:** self-modification flag handling; branch-before-evolve; self-analysis/plan/apply/test; plan-only default.
- **Exit criteria:** `evolve` produces a self-authoring plan on AutoAgent's own tree, gated behind `allow_self_modification`, defaulting to plan-only, executing on an isolated branch when applied.

#### Phase 7 (0.7.0): Plugin System

- **Goal:** Extensibility without bypassing safety.
- **Scope:** Rust plugin traits; WASM plugin support; tool registry; plugin manifest.
- **Exit criteria:** a sample plugin registers and runs entirely through the safety layer.

#### Phase 8 (1.0.0): Stable Release

- **Goal:** Stable, documented, guaranteed-reversible 1.0.
- **Scope:** stable CLI, config schema, plan schema; reversible patches; policy enforcement; audit logging — all frozen.
- **Exit criteria:** schemas stable and documented; full reversibility and policy enforcement verified by the test suite; no breaking changes pending.

### 4.2 Testing Strategy

- **Unit tests:** per module in `autoagent-core` — path guard normalization/escape cases, command guard allow/block matching, snapshot hashing, plan validation, revert restoration. Core is testable without the CLI by design.
- **Integration tests:** drive a temporary sample repo through `init → apply → revert` and `analyze`, asserting on-disk run-folder contracts and that reverts restore byte-for-byte (hash-verified).
- **End-to-end tests:** exercise the real compiled `autoagent` binary against a real throwaway Git workspace — real filesystem, real `cargo`/`git` subprocesses through the command guard, real `.agent/` artifacts — with no mocked layers, asserting the user-visible workflow (e.g. `apply` a plan that edits files and runs `cargo test`, then `revert` and confirm the tree is restored). *Genuine E2E only: if any layer is stubbed, it is labeled an integration test, not E2E.*
- **Security regression tests:** every safety rule (workspace escape, blocked path, blocked command, approval gate) has a test that fails the operation; new safety fixes ship with a regression test.
- **Load/scale tests:** `analyze`/scan benchmarked against a 10k-file fixture to validate the proposed performance targets (§2.2).

### 4.3 Rollout Strategy

- **Versioned releases:** semver per the roadmap (0.1.0 → 1.0.0); each version's exit criteria gate the tag.
- **Feature gating by config, not flags:** dangerous capabilities are off by default in `Autoagent.toml` (`allow_self_modification = false`, approval-before-write/command = true) — users opt in explicitly rather than via hidden flags.
- **Rollback:** because the tool's whole premise is reversibility, "rollback" for *users* is `revert <run-id>`; for *releases*, prior binaries remain installable and config/plan schemas are versioned to stay readable across versions.

### 4.4 Operational Readiness

Before a release is considered production-ready:

- `doctor` passes on a clean machine with only Rust/Cargo/Git installed.
- 100% of the safety regression suite passes; zero out-of-policy writes.
- Every command's default write behavior matches §3.4 (read-only stays read-only).
- Run-folder contract (§9 of source / §3.3 here) is produced completely for every run.
- README + `--help` (Appendix B) document defaults, especially the supervised/approval defaults.

---

## 5. Milestones

> Owner is the solo developer (Ryan O'Boyle) for all milestones; target dates are **TBD** per project decision (no fixed timeline). Sequencing is strict — each milestone depends on the prior engine guarantees holding.

| Milestone | Goal | Exit Criteria | Target Date | Owner |
|-----------|------|---------------|-------------|-------|
| M1 — 0.1.0 Mutation Engine | Safe, reversible apply of JSON plans | `init/doctor/apply/revert` E2E; all runs snapshotted+reversible; 0 out-of-policy writes | TBD | Ryan |
| M2 — 0.2.0 Project Analyzer | Codebase understanding | `analyze` accurate on Rust + JS/TS repos; respects ignore/include/exclude | TBD | Ryan |
| M3 — 0.3.0 Planner Interface | Plan generation (incl. LLM, no mutation authority) | Policy-valid plans; local + cloud provider both work; no egress without opt-in | TBD | Ryan |
| M4 — 0.4.0 Validation Loop | Supervised run + bounded repair | `run` completes; failed validation triggers bounded repair; reversible | TBD | Ryan |
| M5 — 0.5.0 Memory | Cross-run knowledge | Memory persists and informs later runs | TBD | Ryan |
| M6 — 0.6.0 Evolve Mode | Controlled self-authoring | `evolve` plan-only by default, gated, branch-isolated when applied | TBD | Ryan |
| M7 — 0.7.0 Plugin System | Safe extensibility | Sample plugin runs through the safety layer | TBD | Ryan |
| M8 — 1.0.0 Stable Release | Frozen, guaranteed-reversible 1.0 | Schemas stable+documented; reversibility & policy enforcement verified | TBD | Ryan |

### Dependency Graph

```text
M1 (0.1.0 engine) ──► M2 (analyzer) ──► M3 (planner) ──► M4 (validation loop)
       │                                                        │
       │                                                        ▼
       │                                                  M5 (memory)
       │                                                        │
       └────────────── safety guarantees flow forward ─────────┤
                                                                ▼
                                                         M6 (evolve)
                                                                │
                                                                ▼
                                                       M7 (plugins) ──► M8 (1.0.0)
```

M1's safety + reversibility guarantees are a precondition for *every* later milestone — nothing that can write is built before the engine that makes writes reversible.

---

## 6. Success Criteria

> **Status:** These metrics are **proposed/derived**, not user-confirmed (the discovery answer for success metrics was non-specific). Confirming them is tracked as **OQ-1**. They favor the product's identity — safety and reversibility — over adoption, per the local-tool framing.

### 6.1 Launch Metrics

| Metric | Target | Measurement method |
|--------|--------|--------------------|
| Reversibility | 100% of applied runs fully revert to pre-run state | Automated E2E: apply then revert, hash-compare tree |
| Policy integrity | 0 out-of-policy writes / blocked-command executions | Security regression suite + audit-log scan |
| Audit completeness | 100% of file ops & commands recorded in `events.jsonl` | Run-folder contract test |
| MVP functionality | `init/doctor/apply/revert` succeed on sample repos | E2E suite green |
| Self-authoring success (from 0.6.0) | `evolve` produces a valid, policy-clean self-plan that `cargo test` passes after supervised apply | Self-authoring E2E on AutoAgent's own tree |

### 6.2 Ongoing Monitoring

- The per-run `run.json` + `events.jsonl` are the "dashboard": each run is independently inspectable.
- Review cadence: per-milestone exit-criteria review (no fixed calendar; tied to version completion).
- CI is the continuous monitor — fmt/clippy/test/build plus the safety regression suite on every change.

### 6.3 Remediation Triggers

- Any out-of-policy write or blocked-command bypass in CI → **stop-the-line**: fix + regression test before merge (per the project's security-fix discipline).
- A revert that fails to restore the tree → release-blocking; reversibility is the core guarantee.
- `doctor` failing on a supported clean environment → blocks release readiness.

---

## 7. Risks

| ID | Risk | Impact | Likelihood | Mitigation | Contingency |
|----|------|--------|-----------|------------|-------------|
| R-1 | A path/command guard gap lets a write or command escape policy (the core safety promise fails) | Critical | Medium | Default-deny lists; normalize+resolve before evaluating; blocked rules override allows; exhaustive security regression tests | Treat as stop-the-line; patch + regression test before any further work; audit affected runs |
| R-2 | `revert` fails to restore a tree (snapshot incomplete, external drift, partial apply) | Critical | Medium | Snapshot every touched file before write; `before_hash`/`after_hash` drift detection; atomic-failure E2E tests | Block release; surface drift to user instead of overwriting; never auto-prune run history |
| R-3 | LLM-generated plans propose unsafe or wrong operations | High | High | Model produces *plans only*; `plan_validator` + policy engine gate every op; supervised approval default | Keep approval-before-write on; require human review of LLM plans pre-apply |
| R-4 | Source-code egress to a cloud provider leaks secrets/IP | High | Medium | Opt-in only; local-model option; redaction/exclude hook; provider calls audited | Default to local model; ship redaction before enabling cloud providers (gate OQ-4) |
| R-5 | Self-authoring (`evolve`) corrupts AutoAgent's own tree | High | Low | Plan-only default; `allow_self_modification=false`; branch-before-evolve isolation; self-test gate | Operate only on a throwaway branch; revert via snapshots; never apply self-plans on `main` |
| R-6 | Solo-developer bandwidth stalls the 8-milestone roadmap | Medium | Medium | Strict phase ordering delivers a useful, safe MVP (0.1.0) independently of later phases | Ship and stabilize the mutation engine as a standalone tool even if later phases slip |
| R-7 | Dependency/runtime drift (e.g. tree-sitter, async stack) introduced too early adds risk before the engine is solid | Medium | Low | Keep 0.1.0 dependency budget minimal; defer async/network/tree-sitter to the milestones that need them | Pin versions; gate new deps behind the milestone that requires them |

---

## 8. Open Questions

| # | Question | Owner | Due Date |
|---|----------|-------|----------|
| OQ-1 | Confirm or replace the proposed success metrics (§6) and NFR latency targets (§2.2) — the discovery answer was non-specific | Ryan | Before M1 exit |
| OQ-2 | Distribution channel: `cargo install`, tagged GitHub release binaries, or both? | Ryan | Before M8 |
| OQ-3 | Is a future metrics/tracing surface (beyond event logs) wanted, or is the audit trail sufficient long-term? | Ryan | Before M5 |
| OQ-4 | Exact redaction/exclusion policy for code sent to LLM providers (what is stripped, how secrets are detected) | Ryan | Before M3 (gates cloud-provider enablement) |
| OQ-5 | Which specific providers ship first for local (Ollama? llama.cpp?) and cloud (Anthropic, OpenAI, others?) | Ryan | Before M3 |
| OQ-6 | Run-history retention/pruning UX — confirm it stays a manual, never-automatic action | Ryan | Before M1 exit |

---

## Appendices

### Appendix A: Glossary

| Term | Meaning |
|------|---------|
| Controlled self-authoring | AutoAgent operating on its own source tree under explicit policy, supervision, snapshots, and rollback — the product's marquee identity. Explicitly *not* uncontrolled self-replication. |
| Mutation engine | The `autoagent-core` editing+safety pipeline that applies validated `FileOperation`s with snapshot-before-write. |
| Plan (as contract) | A structured JSON document of operations the engine validates and applies; the runtime applies validated contracts rather than free-form edits. |
| Policy engine | The single chokepoint (`safety/`) authorizing every path and command against `Autoagent.toml`. |
| Run folder | `.agent/runs/<run-id>/` — the complete, append-only audit record of one run. |
| Snapshot | A pre-mutation copy of a file in `before/`, enabling reversibility. |
| Supervised mode | Default agent mode requiring approval before writes and commands. |
| Evolve mode | The plan-only-by-default self-authoring workflow, gated by `allow_self_modification`. |

### Appendix B: Official Positioning Phrases (from source Appendix A)

- A Rust-native local agent runtime for safe, reversible, policy-controlled codebase evolution.
- A recursive developer agent for controlled codebase evolution.
- A self-authoring software agent that can inspect, patch, validate, and evolve codebases under explicit user policy.
- **Self-authoring, not uncontrolled self-replicating.**
- The safe mutation engine for autonomous software development workflows.

### Appendix C: Canonical `Autoagent.toml` (reference)

The full reference configuration — `[project]`, `[agent]`, `[workspace]` (include/exclude), `[commands]`, `[safety]` (allowed/blocked write paths and commands), `[memory]`, `[logging]`, `[patches]`, `[runs]` — is specified verbatim in §6 of the source `AutoAgent Technical Specification.md` and is the authoritative schema for M1. Notable defaults: `mode = "supervised"`, `allow_self_modification = false`, `max_steps_per_run = 8`, `require_approval_before_write = true`, `require_approval_before_command = true`.

### Appendix D: CLI Help (draft, from source Appendix B)

```text
AutoAgent
USAGE:  autoagent <COMMAND> [OPTIONS]

COMMANDS:
  init     Initialize AutoAgent in the current workspace
  doctor   Check local system, config, commands, and workspace health
  analyze  Analyze the current project and write a project report
  plan     Create or import a structured implementation plan
  apply    Apply a structured plan through the policy-controlled mutation engine
  run      Plan, apply, validate, and report in a supervised workflow
  evolve   Create a controlled self-authoring plan for AutoAgent itself
  patch    List, show, or save patch artifacts
  revert   Revert a previous AutoAgent run
  memory   Show or manage project memory
  config   Show or validate Autoagent.toml

DEFAULTS:
  - Write operations require approval.
  - Unknown commands require approval.
  - Evolve mode is plan-only unless explicitly applied.
  - Every applied run creates snapshots and logs.
```

### Appendix E: Decision Log

| Decision | Rationale |
|----------|-----------|
| TOML config over JS/JSON | Readable, stable, Rust-native, safe (no executable config). |
| Engine-before-planner ordering | Safety must be correct before intelligence gains power; planner never bypasses the engine. |
| LLM produces plans, never mutations | Keeps the model out of the privileged path; plans are validated contracts. |
| Support both local and main cloud LLM providers | Local-model option avoids code egress; cloud providers (Anthropic/OpenAI) offer capability when opted in. |
| Run history never auto-pruned | The audit trail is a safety feature; deletion is always a deliberate user action. |
| Self-modification off + plan-only by default | "Self-authoring, not self-replicating" enforced in defaults, not just docs. |
