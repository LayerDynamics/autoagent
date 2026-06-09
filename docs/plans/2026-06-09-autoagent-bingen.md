# AutoAgent Binding Generator (`autoagent-bingen`) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use lore:execute to implement this plan task-by-task.
> **Scope guard:** Do ONLY what is listed here. If you discover adjacent issues, note them as a TODO and continue. Do NOT fix them.

**Goal:** Implement `crates/autoagent-bingen` — a single-source-of-truth generator that exposes `autoagent-core`'s full CLI surface to Node.js, Python, and Deno across six backends, with identical safety guarantees to the CLI.
**Architecture:** `bind.rs` is the contract: a declarative surface registry + backend-neutral wrappers (serde-JSON marshaling, `AutoAgentError` mapping, a `CallbackGate` bridging core's `ApprovalGate`, and async via `tokio::Runtime::block_on`). The `bingen` binary (`src/main.rs`) reads the registry and generates the six backend adapters (`src/node/{napi,node_bindgen}.rs`, `src/python/{pyrs,python_bingen}.rs`, `src/deno/{deno_bindgen,ffi}.rs`), the `.d.ts`/`.pyi`/`mod.ts` stubs, `surface.schema.json`, and package scaffolds. Each backend compiles behind a Cargo feature into a loadable native module. Generated files are golden-tested by `bingen check`.
**Tech Stack:** Rust 1.78 (edition 2021), `autoagent-core` (path dep), napi-rs, node-bindgen, pyo3 (abi3), rustpython-vm, deno_bindgen + raw FFI; `serde`/`serde_json`/`tokio`/`camino` (workspace); maturin, npm/napi-cli, JSR for distribution; GitHub Actions matrix.
**Practices:** **Typed-first** (additive serde derives + DTO shapes before marshaling) → **Contract-first** (freeze `bind.rs` registry + `surface.schema.json` before any codegen) → **TDD** (failing test → implement → green) for every task.
**Required skills:** none (Rust crate; not a Claude Code extension/MCP/agent).

**Spec:** `docs/specs/SPEC-2-autoagent-bingen.md` (FR/NFR/risk IDs referenced per task).

---

## Ground truth (verified against the codebase — do NOT re-derive)

These are the exact core entrypoints `bind.rs` wraps. The CLI's `crates/autoagent-cli/src/commands/mod.rs` is the façade to **replicate** (bingen depends on core, NOT cli):

| Surface op | Core call (verified) | Kind | Privilege |
|-----------|----------------------|------|-----------|
| `init` | `runtime::init::init_workspace(&root) -> bool` | sync | mutate (fs) |
| `doctor` | `runtime::doctor::doctor(&root) -> DoctorReport` | sync | read |
| `analyze` | `config::config_schema::AutoAgentConfig::load`, `analysis::project_analyzer::analyze(&root,&cfg) -> ProjectAnalysis`, `analysis::report_writer::write_report` | sync | read(+report write) |
| `plan` | `commands::plan` pattern: `--from` → `plan_reader::read_plan` + `PolicyEngine::from_config` + `plan_validator::validate_plan`; generate → `planner::generate_plan` (async, needs LLM); then `plan_writer::write_plan` | async/sync | read |
| `apply` | `runtime::agent_loop::apply_with_gate(&root,&plan_path,&dyn ApprovalGate) -> String` | sync | mutate |
| `run` | `commands::run` pattern: gate.confirm_write/command, then `run_workflow::run_with_plan` (sync, `--from`) or `run_workflow::run_workflow` (async, LLM) → `RunOutcome` | async/sync | mutate |
| `evolve` | `commands::evolve` pattern: `evolve_workflow::evolve_with_plan` (sync) or `evolve_workflow::evolve_generated` (async) → `EvolveOutcome` | async/sync | mutate |
| `revert` | `runtime::revert::revert(&root,&run_id) -> ()` | sync | mutate |
| `patch.list` | read `.agent/patches/*.patch` (mirror `commands::patch_list`) | sync | read |
| `patch.show` | read `.agent/patches/<run_id>.patch` (mirror `commands::patch_show`) | sync | read |
| `config.show` | `AutoAgentConfig::load` → `toml::to_string_pretty` (mirror `commands::config_show`) | sync | read |
| `memory.show/rebuild/add/remove` | `memory::memory_store::MemoryStore` methods + `memory::project_memory::rebuild_project_memory` | sync | read/mutate(.agent) |
| `tools.list` | `plugins::with_builtins()?.tool_names()` + `plugins::discover_wasm_plugins(&root)` | sync | read |
| `version` | `schema_version::SCHEMA_VERSION` (`u32 = 1`) | sync | read |

**Approval bridge (verified):** `safety::approval_gate::ApprovalGate` has `confirm_write(&self,target:&str)->Result<()>` and `confirm_command(&self,command:&str)->Result<()>`. `AutoGate::allow()`/`deny()` are the non-interactive impls. Denial returns `PolicyError::WriteNotApproved` / `CommandNotApproved`. Bindings implement a `CallbackGate` over a host callback; **no callback + no `approve` flag ⇒ deny** (FR-7/FR-20).

**Serde-derive precondition (verified MISSING):** `DoctorReport`/`Check` (`runtime/doctor.rs`), `RunOutcome`/`RunState` (`runtime/run_workflow.rs`, `runtime/run_state.rs`), `EvolveOutcome` (`runtime/evolve_workflow.rs`), and `AutoAgentError`/`PolicyError` (`error/autoagent_error.rs`) do **not** derive `Serialize`. `ProjectAnalysis`, `Plan`, `ValidationReport` already do. Additive derives only (R-8) — Task B1-T2.

**Error taxonomy (verified):** `AutoAgentError::error_code() -> String` (e.g. `policy.path_escape`) and `exit_code() -> i32` already exist on the type.

---

## Milestone B1 — Registry + Generator + napi-rs read surface (FR-1, FR-2, FR-12, FR-15, FR-16)

### Task B1-T1: Add crate to the workspace

**Files:**
- Modify: `Cargo.toml:3-7` (workspace members)
- Create: `crates/autoagent-bingen/Cargo.toml`

**Step 1: Write the failing test**
Add to `crates/autoagent-bingen/Cargo.toml` later; first prove membership. Run:
`cargo metadata --format-version 1 --no-deps | grep -c autoagent-bingen` → Expected now: `0`

**Step 2: Add the member**
Edit `Cargo.toml` members array to include `"crates/autoagent-bingen"`:
```toml
members = [
    "crates/autoagent-core",
    "crates/autoagent-cli",
    "crates/autoagent-plugin-sdk",
    "crates/autoagent-bingen",
]
```

**Step 3: Write `crates/autoagent-bingen/Cargo.toml`**
```toml
[package]
name = "autoagent-bingen"
version.workspace = true
edition.workspace = true
license.workspace = true
build = "build.rs"

[lib]
# Backend adapters compile into a loadable native module; `rlib` lets the
# `bingen` bin and tests link the registry.
crate-type = ["cdylib", "rlib"]
path = "src/lib.rs"

[[bin]]
name = "bingen"
path = "src/main.rs"

[features]
# Exactly one backend per build (mutually exclusive at link time).
default = []
node-napi = ["dep:napi", "dep:napi-derive"]
node-bindgen = ["dep:node-bindgen"]
py-pyo3 = ["dep:pyo3", "dep:pyo3-async-runtimes"]
py-rustpython = ["dep:rustpython-vm"]
deno-bindgen = ["dep:deno_bindgen"]
deno-ffi = []

[dependencies]
autoagent-core = { path = "../autoagent-core" }
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
camino.workspace = true
tokio.workspace = true
toml.workspace = true
chrono.workspace = true
# Versions verified against crates.io 2026-06-09 (latest stable, compatible with
# Rust 1.96 / Node 24 / Python 3.14 / Deno 2.8 installed on this machine).
napi = { version = "3", default-features = false, features = ["napi8", "tokio_rt"], optional = true }
napi-derive = { version = "3", optional = true }
node-bindgen = { version = "6", optional = true }
pyo3 = { version = "0.28", features = ["abi3-py39", "extension-module"], optional = true }
# Async Python bridge: pyo3-asyncio is dead (pins to pyo3 0.20); use the live
# successor, version-locked to pyo3 0.28. Pulled in only for run/evolve in B3-T3.
pyo3-async-runtimes = { version = "0.28", features = ["tokio-runtime"], optional = true }
rustpython-vm = { version = "0.5", optional = true }
deno_bindgen = { version = "0.8", optional = true }

[build-dependencies]
napi-build = "2"

[dev-dependencies]
tempfile.workspace = true
```

**Step 4: Verify membership**
`cargo metadata --format-version 1 --no-deps | grep -c autoagent-bingen` → Expected: `1`
(Build will fail until B1-T3 creates `src/lib.rs`/`src/main.rs`/`build.rs` — that is expected here.)

**Step 5: Commit**
`git add Cargo.toml crates/autoagent-bingen/Cargo.toml && git commit -m "build(bingen): add crate to workspace with backend feature matrix"`

---

### Task B1-T2: Typed-first — additive serde derives on boundary types (R-8, FR-8)

**Files:**
- Modify: `crates/autoagent-core/src/runtime/doctor.rs:7,14` (`Check`, `DoctorReport`)
- Modify: `crates/autoagent-core/src/runtime/run_state.rs:6` (`RunState`)
- Modify: `crates/autoagent-core/src/runtime/run_workflow.rs:22` (`RunOutcome`)
- Modify: `crates/autoagent-core/src/runtime/evolve_workflow.rs:16` (`EvolveOutcome`)
- Test: `crates/autoagent-core/src/runtime/doctor.rs` (inline `#[cfg(test)]`)

**Step 1: Write the failing test** (append to `doctor.rs` tests module)
```rust
#[test]
fn doctor_report_serializes_to_json() {
    let r = DoctorReport { checks: vec![Check { name: "x".into(), ok: true, detail: "d".into() }] };
    let j = serde_json::to_string(&r).expect("DoctorReport must serialize");
    assert!(j.contains("\"ok\":true"));
}
```

**Step 2: Run to verify it fails**
`cargo test -p autoagent-core doctor_report_serializes_to_json` → Expected: FAIL (no `Serialize`)

**Step 3: Add additive derives** (behavior-preserving; add `serde::{Serialize, Deserialize}` to each derive list, e.g.)
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Check { /* unchanged fields */ }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DoctorReport { /* unchanged fields */ }
```
Repeat for `RunState`, `RunOutcome`, `EvolveOutcome` (add `serde::Serialize, serde::Deserialize` to their existing `#[derive(...)]`). `RunOutcome` nests `RunState` + `ValidationReport` (already serde) — both now satisfy bounds.

**Step 4: Run to verify it passes + no regressions**
`cargo test -p autoagent-core` → Expected: PASS (all existing tests + new one)

**Step 5: Commit**
`git add crates/autoagent-core/src/runtime/ && git commit -m "feat(core): additive serde derives on DoctorReport/RunOutcome/RunState/EvolveOutcome (bingen boundary, no behavior change)"`

---

### Task B1-T3: Crate skeleton — `lib.rs`, `build.rs`, `main.rs` stubs that compile

**Files:**
- Create: `crates/autoagent-bingen/src/lib.rs`
- Create: `crates/autoagent-bingen/build.rs`
- Create: `crates/autoagent-bingen/src/main.rs`
- Create: `crates/autoagent-bingen/src/node/mod.rs`, `src/python/mod.rs`, `src/deno/mod.rs`

**Step 1: Write `src/lib.rs`** (`bind.rs` lives at crate root, included via `#[path]`)
```rust
//! autoagent-bingen — generates Python/Node/Deno bindings for autoagent-core
//! from a single surface registry. See docs/specs/SPEC-2-autoagent-bingen.md.

#[path = "../bind.rs"]
pub mod bind;

pub mod node;
pub mod python;
pub mod deno;
```

**Step 2: Write `src/node/mod.rs`, `src/python/mod.rs`, `src/deno/mod.rs`** (feature-gated re-exports; empty until their backends are generated)
```rust
// src/node/mod.rs
#[cfg(feature = "node-napi")]
pub mod napi;
#[cfg(feature = "node-bindgen")]
pub mod node_bindgen;
```
```rust
// src/python/mod.rs
#[cfg(feature = "py-pyo3")]
pub mod pyrs;
#[cfg(feature = "py-rustpython")]
pub mod python_bingen;
```
```rust
// src/deno/mod.rs
#[cfg(feature = "deno-bindgen")]
pub mod deno_bindgen;
#[cfg(feature = "deno-ffi")]
pub mod ffi;
```

**Step 3: Write `build.rs`** (napi build only when its feature is on; generation check wired in B1-T7)
```rust
fn main() {
    #[cfg(feature = "node-napi")]
    napi_build::setup();
    println!("cargo:rerun-if-changed=bind.rs");
}
```

**Step 4: Write minimal `src/main.rs`** (real subcommand dispatch; bodies filled in later tasks)
```rust
//! `bingen` — the binding generator CLI.
use std::process::ExitCode;

fn main() -> ExitCode {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    match cmd.as_str() {
        "generate" => autoagent_bingen_gen::generate().map_or(ExitCode::FAILURE, |_| ExitCode::SUCCESS),
        "check" => autoagent_bingen_gen::check().map_or(ExitCode::FAILURE, |_| ExitCode::SUCCESS),
        "smoke" => autoagent_bingen_gen::smoke().map_or(ExitCode::FAILURE, |_| ExitCode::SUCCESS),
        other => { eprintln!("usage: bingen [generate|check|smoke] (got {other:?})"); ExitCode::FAILURE }
    }
}

// Generator implementation module (sibling file).
#[path = "gen/mod.rs"]
mod autoagent_bingen_gen;
```

**Step 5: Create generator placeholder so it compiles** — `crates/autoagent-bingen/src/gen/mod.rs`:
```rust
//! Code generation entrypoints (filled in B1-T5..T7).
use anyhow::Result;
pub fn generate() -> Result<()> { crate_generate() }
pub fn check() -> Result<()> { anyhow::bail!("check not yet implemented") }
pub fn smoke() -> Result<()> { anyhow::bail!("smoke not yet implemented") }
fn crate_generate() -> Result<()> { anyhow::bail!("generate not yet implemented") }
```
Add `extern crate autoagent_bingen;` access by referencing the lib from the bin: in `main.rs` the bin already links the lib crate as `autoagent_bingen` — use `autoagent_bingen::bind` inside `gen/mod.rs` via `use autoagent_bingen::bind;` (the bin and lib share the package name).

**Step 6: Create empty `bind.rs`** at `crates/autoagent-bingen/bind.rs` with a doc comment so `#[path]` resolves:
```rust
//! Surface registry + neutral wrappers (filled in B1-T4).
```

**Step 7: Run to verify it compiles**
`cargo build -p autoagent-bingen` → Expected: PASS (no features; cdylib+rlib+bin build; generator returns errors at runtime, which is fine)

**Step 8: Commit**
`git add crates/autoagent-bingen && git commit -m "feat(bingen): crate skeleton (lib/build/main + module gates compile)"`

---

### Task B1-T4: Contract-first — the surface registry in `bind.rs`

**Files:**
- Modify: `crates/autoagent-bingen/bind.rs`
- Test: `crates/autoagent-bingen/tests/registry.rs`

**Step 1: Write the failing test** — `crates/autoagent-bingen/tests/registry.rs`
```rust
use autoagent_bingen::bind::{SURFACE, Kind, Privilege};

#[test]
fn registry_covers_full_cli_parity() {
    let names: Vec<&str> = SURFACE.iter().map(|s| s.name).collect();
    for required in ["init","doctor","analyze","plan","apply","run","evolve",
                     "revert","patch_list","patch_show","config_show",
                     "memory_show","tools_list","version"] {
        assert!(names.contains(&required), "missing surface symbol: {required}");
    }
}

#[test]
fn mutating_ops_marked_mutate() {
    for s in SURFACE.iter().filter(|s| ["apply","run","evolve","revert"].contains(&s.name)) {
        assert!(matches!(s.privilege, Privilege::Mutate), "{} must be Mutate", s.name);
    }
}

#[test]
fn async_ops_marked_async() {
    for s in SURFACE.iter().filter(|s| ["run","evolve"].contains(&s.name)) {
        assert!(matches!(s.kind, Kind::Async), "{} must be Async", s.name);
    }
}
```

**Step 2: Run to verify it fails**
`cargo test -p autoagent-bingen --test registry` → Expected: FAIL (no `SURFACE`)

**Step 3: Implement the registry** in `bind.rs` (the contract; drives all codegen + schema)
```rust
//! Surface registry + neutral wrappers — the single source of truth.

#[derive(Clone, Copy, Debug)]
pub enum Kind { Sync, Async }
#[derive(Clone, Copy, Debug)]
pub enum Privilege { Read, Mutate }

#[derive(Clone, Copy, Debug)]
pub struct Arg { pub name: &'static str, pub ty: &'static str }

#[derive(Clone, Copy, Debug)]
pub struct Symbol {
    pub name: &'static str,
    pub kind: Kind,
    pub privilege: Privilege,
    pub args: &'static [Arg],
    pub returns: &'static str, // TS/py type name or "void"
    pub doc: &'static str,
}

const S_ROOT: Arg = Arg { name: "root", ty: "string" };

pub static SURFACE: &[Symbol] = &[
    Symbol { name: "version", kind: Kind::Sync, privilege: Privilege::Read,
             args: &[], returns: "number", doc: "Schema version this build supports." },
    Symbol { name: "doctor", kind: Kind::Sync, privilege: Privilege::Read,
             args: &[S_ROOT], returns: "DoctorReport", doc: "System/workspace health checks." },
    Symbol { name: "analyze", kind: Kind::Sync, privilege: Privilege::Read,
             args: &[S_ROOT], returns: "ProjectAnalysis", doc: "Analyze the project; write report." },
    Symbol { name: "init", kind: Kind::Sync, privilege: Privilege::Mutate,
             args: &[S_ROOT], returns: "boolean", doc: "Initialize Autoagent.toml + .agent/." },
    Symbol { name: "plan", kind: Kind::Async, privilege: Privilege::Read,
             args: &[S_ROOT, Arg{name:"objective",ty:"string"}, Arg{name:"from",ty:"string | null"}],
             returns: "string", doc: "Generate or import+validate a plan; returns plan path." },
    Symbol { name: "apply", kind: Kind::Sync, privilege: Privilege::Mutate,
             args: &[S_ROOT, Arg{name:"plan_path",ty:"string"}, Arg{name:"opts",ty:"ApproveOpts"}],
             returns: "string", doc: "Apply a plan through the policy engine; returns run id." },
    Symbol { name: "run", kind: Kind::Async, privilege: Privilege::Mutate,
             args: &[S_ROOT, Arg{name:"objective",ty:"string"}, Arg{name:"from",ty:"string | null"}, Arg{name:"opts",ty:"ApproveOpts"}],
             returns: "RunOutcome", doc: "Supervised plan→apply→validate→repair→report." },
    Symbol { name: "evolve", kind: Kind::Async, privilege: Privilege::Mutate,
             args: &[S_ROOT, Arg{name:"objective",ty:"string"}, Arg{name:"from",ty:"string | null"}, Arg{name:"apply",ty:"boolean"}],
             returns: "EvolveOutcome", doc: "Controlled self-authoring plan (apply gated)." },
    Symbol { name: "revert", kind: Kind::Sync, privilege: Privilege::Mutate,
             args: &[S_ROOT, Arg{name:"run_id",ty:"string"}], returns: "void", doc: "Revert a previous run." },
    Symbol { name: "patch_list", kind: Kind::Sync, privilege: Privilege::Read,
             args: &[S_ROOT], returns: "string[]", doc: "List patch artifact run ids." },
    Symbol { name: "patch_show", kind: Kind::Sync, privilege: Privilege::Read,
             args: &[S_ROOT, Arg{name:"run_id",ty:"string"}], returns: "string", doc: "Show a patch body." },
    Symbol { name: "config_show", kind: Kind::Sync, privilege: Privilege::Read,
             args: &[S_ROOT], returns: "string", doc: "Render Autoagent.toml." },
    Symbol { name: "memory_show", kind: Kind::Sync, privilege: Privilege::Read,
             args: &[S_ROOT], returns: "MemorySummary", doc: "Project memory summary." },
    Symbol { name: "tools_list", kind: Kind::Sync, privilege: Privilege::Read,
             args: &[S_ROOT], returns: "string[]", doc: "Registered plugin tools." },
];
```

**Step 4: Run to verify it passes**
`cargo test -p autoagent-bingen --test registry` → Expected: PASS

**Step 5: Commit**
`git add crates/autoagent-bingen/bind.rs crates/autoagent-bingen/tests/registry.rs && git commit -m "feat(bingen): surface registry contract (full CLI parity, FR-1/FR-4)"`

---

### Task B1-T5: Neutral wrappers in `bind.rs` (read surface: version/doctor/analyze + error payload)

**Files:**
- Modify: `crates/autoagent-bingen/bind.rs`
- Test: `crates/autoagent-bingen/tests/wrappers.rs`

**Step 1: Write the failing test** — `tests/wrappers.rs`
```rust
use autoagent_bingen::bind;

#[test]
fn version_returns_schema_version_json() {
    let j = bind::version().unwrap();
    assert_eq!(j.trim(), "1"); // SCHEMA_VERSION
}

#[test]
fn doctor_returns_serialized_report() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let j = bind::doctor(root).unwrap();
    assert!(j.contains("\"checks\""));
}

#[test]
fn error_payload_carries_code_and_exit() {
    // analyze on a non-initialized dir surfaces a config/workspace error.
    let dir = tempfile::tempdir().unwrap();
    let err = bind::analyze(dir.path().to_str().unwrap()).unwrap_err();
    assert!(!err.code.is_empty());
    assert!(err.exit_code >= 1);
}
```

**Step 2: Run to verify it fails**
`cargo test -p autoagent-bingen --test wrappers` → Expected: FAIL (no wrappers)

**Step 3: Implement wrappers + error payload** (append to `bind.rs`)
```rust
use autoagent_core::error::AutoAgentError;
use camino::Utf8Path;

/// Backend-neutral error: stable code + numeric exit + message (FR-8).
#[derive(Debug, Clone, serde::Serialize)]
pub struct BindError { pub code: String, pub exit_code: i32, pub message: String }
impl From<AutoAgentError> for BindError {
    fn from(e: AutoAgentError) -> Self {
        BindError { code: e.error_code(), exit_code: e.exit_code(), message: e.to_string() }
    }
}
pub type BindResult = std::result::Result<String, BindError>;

fn utf8(root: &str) -> Result<&Utf8Path, BindError> {
    Utf8Path::from_path(std::path::Path::new(root))
        .ok_or_else(|| BindError { code: "workspace".into(), exit_code: 2, message: "non-utf8 path".into() })
}

pub fn version() -> BindResult {
    Ok(autoagent_core::schema_version::SCHEMA_VERSION.to_string())
}

pub fn doctor(root: &str) -> BindResult {
    let report = autoagent_core::runtime::doctor::doctor(utf8(root)?);
    Ok(serde_json::to_string(&report).map_err(serde_err)?)
}

pub fn analyze(root: &str) -> BindResult {
    let root = utf8(root)?;
    let cfg = autoagent_core::config::config_schema::AutoAgentConfig::load(root)?;
    let analysis = autoagent_core::analysis::project_analyzer::analyze(root, &cfg)?;
    autoagent_core::analysis::report_writer::write_report(root, &analysis)?;
    Ok(serde_json::to_string(&analysis).map_err(serde_err)?)
}

fn serde_err(e: serde_json::Error) -> BindError {
    BindError { code: "serde".into(), exit_code: 1, message: e.to_string() }
}
```

**Step 4: Run to verify it passes**
`cargo test -p autoagent-bingen --test wrappers` → Expected: PASS

**Step 5: Commit**
`git add crates/autoagent-bingen/bind.rs crates/autoagent-bingen/tests/wrappers.rs && git commit -m "feat(bingen): neutral read wrappers + BindError mapping (FR-8)"`

---

### Task B1-T6: Generator — emit napi backend + `.d.ts` + `surface.schema.json`

**Files:**
- Create: `crates/autoagent-bingen/src/gen/emit.rs`
- Modify: `crates/autoagent-bingen/src/gen/mod.rs`
- Test: `crates/autoagent-bingen/tests/generate.rs`

**Step 1: Write the failing test** — `tests/generate.rs`
```rust
use autoagent_bingen::gen;

#[test]
fn generate_emits_napi_and_dts_and_schema() {
    let out = gen::render_all(); // pure: returns map path->content, no fs writes
    assert!(out.get("src/node/napi.rs").unwrap().contains("#[napi]"));
    assert!(out.get("src/node/napi.rs").unwrap().contains("DO NOT EDIT"));
    assert!(out.get("dist/index.d.ts").unwrap().contains("export function doctor"));
    let schema = out.get("schema/surface.schema.json").unwrap();
    assert!(schema.contains("\"name\": \"run\""));
    assert!(schema.contains("\"privilege\": \"mutate\""));
}
```

**Step 2: Run to verify it fails**
`cargo test -p autoagent-bingen --test generate` → Expected: FAIL (no `gen::render_all`)

**Step 3: Implement the emitter** — `src/gen/emit.rs` (pure functions from `SURFACE`)
```rust
use autoagent_bingen::bind::{Kind, Privilege, Symbol, SURFACE};
use std::collections::BTreeMap;

const HEADER: &str = "// DO NOT EDIT — generated by autoagent-bingen from bind.rs.\n";

pub fn render_all() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("src/node/napi.rs".into(), napi_backend());
    m.insert("dist/index.d.ts".into(), dts());
    m.insert("schema/surface.schema.json".into(), schema_json());
    m
}

fn priv_str(p: Privilege) -> &'static str { match p { Privilege::Read => "read", Privilege::Mutate => "mutate" } }
fn kind_str(k: Kind) -> &'static str { match k { Kind::Sync => "sync", Kind::Async => "async" } }

fn schema_json() -> String {
    let syms: Vec<String> = SURFACE.iter().map(|s| {
        let args: Vec<String> = s.args.iter()
            .map(|a| format!("{{ \"name\": \"{}\", \"type\": \"{}\" }}", a.name, a.ty)).collect();
        format!("    {{ \"name\": \"{}\", \"kind\": \"{}\", \"privilege\": \"{}\", \"args\": [{}], \"returns\": \"{}\" }}",
                s.name, kind_str(s.kind), priv_str(s.privilege), args.join(", "), s.returns)
    }).collect();
    format!("{{\n  \"version\": \"1.0.0\",\n  \"symbols\": [\n{}\n  ]\n}}\n", syms.join(",\n"))
}

fn dts() -> String {
    let mut out = String::from("// DO NOT EDIT — generated by autoagent-bingen.\n");
    out.push_str("export class AutoAgentError extends Error { code: string; exitCode: number; }\n");
    for s in SURFACE {
        let params: Vec<String> = s.args.iter().map(|a| format!("{}: {}", a.name, a.ty)).collect();
        let ret = if s.returns == "void" { "void".into() } else { s.returns.to_string() };
        let ret = if matches!(s.kind, Kind::Async) { format!("Promise<{ret}>") } else { ret };
        out.push_str(&format!("/** {} */\nexport function {}({}): {};\n", s.doc, camel(s.name), params.join(", "), ret));
    }
    out
}

fn napi_backend() -> String {
    let mut out = String::from(HEADER);
    out.push_str("use napi_derive::napi;\nuse crate::bind;\n\n");
    out.push_str(NAPI_ERR_GLUE);
    for s in SURFACE {
        out.push_str(&napi_fn(s));
    }
    out
}

const NAPI_ERR_GLUE: &str = r#"fn to_napi(e: bind::BindError) -> napi::Error {
    napi::Error::new(napi::Status::GenericFailure, format!("[{}|{}] {}", e.code, e.exit_code, e.message))
}
fn js<T: serde::de::DeserializeOwned>(j: String) -> napi::Result<T> {
    serde_json::from_str(&j).map_err(|e| napi::Error::from_reason(e.to_string()))
}
"#;

fn napi_fn(s: &Symbol) -> String {
    // Read/sync example for B1: version, doctor, analyze. Mutating/async added in B3.
    match (s.name, kind_str(s.kind)) {
        ("version", _) => "#[napi]\npub fn version() -> napi::Result<u32> { bind::version().map(|v| v.trim().parse().unwrap_or(0)).map_err(to_napi) }\n".into(),
        ("doctor", _) => "#[napi(ts_return_type=\"DoctorReport\")]\npub fn doctor(root: String) -> napi::Result<serde_json::Value> { let j = bind::doctor(&root).map_err(to_napi)?; js(j) }\n".into(),
        ("analyze", _) => "#[napi(ts_return_type=\"ProjectAnalysis\")]\npub fn analyze(root: String) -> napi::Result<serde_json::Value> { let j = bind::analyze(&root).map_err(to_napi)?; js(j) }\n".into(),
        _ => String::new(), // remaining symbols generated in later milestones
    }
}

fn camel(snake: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in snake.chars() {
        if c == '_' { up = true; } else if up { out.push(c.to_ascii_uppercase()); up = false; } else { out.push(c); }
    }
    out
}
```

**Step 4: Wire `gen/mod.rs`** to expose `render_all` + real `generate()`
```rust
//! Code generation entrypoints.
use anyhow::{Context, Result};
use std::path::Path;
#[path = "emit.rs"]
mod emit;
pub use emit::render_all;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

pub fn generate() -> Result<()> {
    for (rel, content) in render_all() {
        let path = Path::new(ROOT).join(&rel);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, content).with_context(|| format!("write {rel}"))?;
        println!("generated {rel}");
    }
    Ok(())
}
pub fn check() -> Result<()> {
    let mut drift = Vec::new();
    for (rel, content) in render_all() {
        let path = Path::new(ROOT).join(&rel);
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        if on_disk != content { drift.push(rel); }
    }
    if drift.is_empty() { println!("no drift"); Ok(()) }
    else { anyhow::bail!("generated files out of date (run `bingen generate`): {drift:?}") }
}
pub fn smoke() -> Result<()> { anyhow::bail!("smoke implemented in B1-T8") }
```
Also expose `pub mod gen;` from `src/lib.rs` so tests can call it:
```rust
#[path = "gen/mod.rs"]
pub mod gen;
```
And remove the duplicate `#[path = "gen/mod.rs"] mod ...` from `main.rs`; instead `use autoagent_bingen::gen;` in `main.rs`.

**Step 5: Run to verify it passes**
`cargo test -p autoagent-bingen --test generate` → Expected: PASS

**Step 6: Generate the files for real + commit**
`cargo run -p autoagent-bingen --bin bingen -- generate` → writes `src/node/napi.rs`, `dist/index.d.ts`, `schema/surface.schema.json`
`git add crates/autoagent-bingen && git commit -m "feat(bingen): generator emits napi backend + .d.ts + surface schema (FR-2/FR-11)"`

---

### Task B1-T7: Drift guard — `bingen check` test + wire into build (FR-15, R-2)

**Files:**
- Test: `crates/autoagent-bingen/tests/drift.rs`

**Step 1: Write the failing test** — `tests/drift.rs`
```rust
use autoagent_bingen::gen;
#[test]
fn generated_files_match_committed_golden() {
    // render_all() output must equal what's on disk (committed). Fails on drift.
    gen::check().expect("generated files are out of date — run `bingen generate`");
}
```

**Step 2: Run to verify it passes** (files were generated+committed in B1-T6)
`cargo test -p autoagent-bingen --test drift` → Expected: PASS
Then prove it *catches* drift: temporarily append a space to `dist/index.d.ts`, rerun → Expected: FAIL; revert the space → PASS.

**Step 3: Commit**
`git add crates/autoagent-bingen/tests/drift.rs && git commit -m "test(bingen): drift guard fails when generated files diverge (FR-15)"`

---

### Task B1-T8: napi build + `bingen smoke` (FR-12, FR-16)

**Files:**
- Modify: `crates/autoagent-bingen/src/gen/mod.rs` (`smoke`)
- Create: `crates/autoagent-bingen/__test__/smoke.mjs`
- Create: `crates/autoagent-bingen/package.json` (minimal, for the smoke build)

**Step 1: Implement `smoke()`** — builds the napi addon and runs a Node script calling `doctor`
```rust
pub fn smoke() -> Result<()> {
    // Build the cdylib with the napi feature, then load it from Node.
    let status = std::process::Command::new("cargo")
        .args(["build", "-p", "autoagent-bingen", "--features", "node-napi", "--release"])
        .status()?;
    anyhow::ensure!(status.success(), "napi build failed");
    let node = std::process::Command::new("node")
        .arg(Path::new(ROOT).join("__test__/smoke.mjs"))
        .status()?;
    anyhow::ensure!(node.success(), "node smoke failed");
    Ok(())
}
```

**Step 2: Write `__test__/smoke.mjs`** (loads the built `.node`/cdylib via require of the platform lib)
```javascript
// Loads the freshly built cdylib and exercises a non-mutating call.
import { createRequire } from "module";
const require = createRequire(import.meta.url);
// napi-rs names the artifact per platform; resolve the workspace target dir.
const path = require("path");
const libDir = path.resolve(process.cwd(), "../../target/release");
const ext = process.platform === "darwin" ? "dylib" : process.platform === "win32" ? "dll" : "so";
const addon = require(path.join(libDir, `libautoagent_bingen.${ext}`));
const report = addon.doctor(process.cwd());
if (!report || !Array.isArray(report.checks)) { console.error("doctor() bad shape", report); process.exit(1); }
console.log("smoke ok:", report.checks.length, "checks");
```
> TODO(B5): napi-cli renames the cdylib to `*.node` per platform; the smoke loader uses the raw cdylib name for now. Replace with the napi loader shim in B5-T2.

**Step 3: Run the smoke test**
`cargo run -p autoagent-bingen --bin bingen -- smoke` → Expected: `smoke ok: N checks` and exit 0
(Requires Node ≥ 18 on PATH; if napi symbol export needs `#[napi]` module registration, ensure `napi_build::setup()` ran — it does via `build.rs` under the feature.)

**Step 4: Commit**
`git add crates/autoagent-bingen && git commit -m "feat(bingen): napi build + node smoke harness (FR-12)"`

**B1 exit criteria (verify before B2):** `cargo test -p autoagent-bingen` green; `cargo run -p autoagent-bingen -- generate` clean; `bingen check` no drift; `bingen smoke` loads the addon and calls `doctor()` from Node.

---

## Milestone B2 — pyo3 + full read surface (FR-3 Python, FR-9 `.pyi`)

### Task B2-T1: Extend registry wrappers — plan/patch/config/memory/tools (read surface)

**Files:**
- Modify: `crates/autoagent-bingen/bind.rs`
- Test: `crates/autoagent-bingen/tests/wrappers.rs` (extend)

**Step 1: Write failing tests** (append to `tests/wrappers.rs`)
```rust
#[test]
fn config_show_renders_toml_after_init() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    bind::init(root).unwrap();                 // writes Autoagent.toml
    let toml = bind::config_show(root).unwrap();
    assert!(toml.contains("[agent]"));
}
#[test]
fn patch_list_empty_is_json_array() {
    let dir = tempfile::tempdir().unwrap();
    let j = bind::patch_list(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(j.trim(), "[]");
}
```

**Step 2: Run to verify failure**
`cargo test -p autoagent-bingen --test wrappers` → Expected: FAIL

**Step 3: Implement the read wrappers** in `bind.rs`, mirroring `commands/mod.rs` but returning JSON (no `println!`):
```rust
pub fn init(root: &str) -> BindResult {
    let wrote = autoagent_core::runtime::init::init_workspace(utf8(root)?)?;
    Ok(wrote.to_string())
}
pub fn config_show(root: &str) -> BindResult {
    let cfg = autoagent_core::config::config_schema::AutoAgentConfig::load(utf8(root)?)?;
    toml::to_string_pretty(&cfg).map_err(|e| BindError { code: "config".into(), exit_code: 2, message: e.to_string() })
}
pub fn patch_list(root: &str) -> BindResult {
    let dir = utf8(root)?.join(".agent/patches");
    let mut names: Vec<String> = std::fs::read_dir(dir.as_std_path())
        .map(|rd| rd.filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with(".patch"))
            .map(|n| n.trim_end_matches(".patch").to_string()).collect())
        .unwrap_or_default();
    names.sort();
    Ok(serde_json::to_string(&names).map_err(serde_err)?)
}
pub fn patch_show(root: &str, run_id: &str) -> BindResult {
    let path = utf8(root)?.join(".agent/patches").join(format!("{run_id}.patch"));
    std::fs::read_to_string(path.as_std_path())
        .map_err(|_| BindError { code: "revert".into(), exit_code: 7, message: format!("no patch for run {run_id}") })
}
pub fn memory_show(root: &str) -> BindResult { /* build MemorySummary struct from MemoryStore, serialize */
    let root = utf8(root)?;
    let cfg = autoagent_core::config::config_schema::AutoAgentConfig::load(root)?;
    let store = autoagent_core::memory::memory_store::MemoryStore::new(root.join(&cfg.memory.directory));
    let pm = store.load_project()?;
    let decisions = store.load_decisions()?;
    let summary = serde_json::json!({
        "name": pm.name, "language": pm.language,
        "source_file_count": pm.source_file_count,
        "decisions": decisions.len(),
    });
    Ok(summary.to_string())
}
pub fn tools_list(root: &str) -> BindResult {
    let mut names = autoagent_core::plugins::with_builtins()?.tool_names();
    for m in autoagent_core::plugins::discover_wasm_plugins(utf8(root)?) { names.push(m.name); }
    Ok(serde_json::to_string(&names).map_err(serde_err)?)
}
```

**Step 4: Run to verify pass**
`cargo test -p autoagent-bingen --test wrappers` → Expected: PASS

**Step 5: Commit**
`git add crates/autoagent-bingen/bind.rs crates/autoagent-bingen/tests/wrappers.rs && git commit -m "feat(bingen): read wrappers for init/config/patch/memory/tools (FR-4 read surface)"`

---

### Task B2-T2: Generator — emit pyo3 backend + `.pyi`

**Files:**
- Modify: `crates/autoagent-bingen/src/gen/emit.rs`
- Test: `crates/autoagent-bingen/tests/generate.rs` (extend)

**Step 1: Write failing test** (append to `tests/generate.rs`)
```rust
#[test]
fn generate_emits_pyo3_and_pyi() {
    let out = autoagent_bingen::gen::render_all();
    assert!(out.get("src/python/pyrs.rs").unwrap().contains("#[pyfunction]"));
    assert!(out.get("src/python/pyrs.rs").unwrap().contains("#[pymodule]"));
    assert!(out.get("python/autoagent/__init__.pyi").unwrap().contains("def doctor"));
}
```

**Step 2: Run to verify failure**
`cargo test -p autoagent-bingen --test generate` → Expected: FAIL

**Step 3: Add `pyo3_backend()` + `pyi()` to `emit.rs`** and register them in `render_all()`:
```rust
// in render_all():
m.insert("src/python/pyrs.rs".into(), pyo3_backend());
m.insert("python/autoagent/__init__.pyi".into(), pyi());
```
```rust
fn pyo3_backend() -> String {
    let mut out = String::from(HEADER);
    out.push_str("use pyo3::prelude::*;\nuse crate::bind;\n\n");
    out.push_str(PYO3_ERR_GLUE);
    let mut adds = String::new();
    for s in SURFACE {
        if let Some(f) = pyo3_fn(s) { out.push_str(&f);
            adds.push_str(&format!("    m.add_function(wrap_pyfunction!({}, m)?)?;\n", s.name)); }
    }
    out.push_str(&format!("#[pymodule]\nfn autoagent(_py: Python, m: &PyModule) -> PyResult<()> {{\n{adds}    Ok(())\n}}\n"));
    out
}
const PYO3_ERR_GLUE: &str = r#"pyo3::create_exception!(autoagent, AutoAgentError, pyo3::exceptions::PyException);
fn to_py(e: bind::BindError) -> PyErr { AutoAgentError::new_err(format!("[{}|{}] {}", e.code, e.exit_code, e.message)) }
"#;
fn pyo3_fn(s: &Symbol) -> Option<String> {
    match s.name {
        "version" => Some("#[pyfunction]\nfn version() -> PyResult<u32> { bind::version().map(|v| v.trim().parse().unwrap_or(0)).map_err(to_py) }\n".into()),
        "doctor"|"analyze"|"config_show"|"patch_list"|"memory_show"|"tools_list" =>
            Some(format!("#[pyfunction]\nfn {n}(root: String) -> PyResult<String> {{ bind::{n}(&root).map_err(to_py) }}\n", n=s.name)),
        _ => None, // mutating/async added in B3
    }
}
fn pyi() -> String {
    let mut out = String::from("# DO NOT EDIT — generated by autoagent-bingen.\n");
    for s in SURFACE {
        let params: Vec<String> = s.args.iter().map(|a| format!("{}: str", a.name)).collect();
        out.push_str(&format!("def {}({}) -> object: ...\n", s.name, params.join(", ")));
    }
    out
}
```

**Step 4: Run to verify pass + regenerate**
`cargo test -p autoagent-bingen --test generate` → Expected: PASS
`cargo run -p autoagent-bingen -- generate` (refresh golden) then `cargo test -p autoagent-bingen --test drift` → Expected: PASS

**Step 5: Commit**
`git add crates/autoagent-bingen && git commit -m "feat(bingen): generate pyo3 backend + .pyi stubs (FR-3/FR-9)"`

---

### Task B2-T3: pyo3 build + Python smoke (pytest)

**Files:**
- Modify: `crates/autoagent-bingen/src/gen/mod.rs` (`smoke` → also build+test Python)
- Create: `crates/autoagent-bingen/pyproject.toml` (maturin)
- Create: `crates/autoagent-bingen/tests_py/test_smoke.py`

**Step 1: Write `pyproject.toml`** (maturin, abi3)
```toml
[build-system]
requires = ["maturin>=1.5,<2"]
build-backend = "maturin"

[project]
name = "autoagent"
requires-python = ">=3.9"
dynamic = ["version"]

[tool.maturin]
features = ["py-pyo3"]
module-name = "autoagent"
```

**Step 2: Write `tests_py/test_smoke.py`**
```python
import autoagent
def test_doctor_and_version(tmp_path):
    assert autoagent.version() == 1
    report = autoagent.doctor(str(tmp_path))  # JSON string
    import json
    assert "checks" in json.loads(report)
```

**Step 3: Build + run** (maturin develop into a venv)
`cd crates/autoagent-bingen && python3 -m venv .venv && . .venv/bin/activate && pip install maturin pytest && maturin develop --features py-pyo3 && pytest tests_py -q`
→ Expected: 1 passed
> **Python 3.14 note (this machine):** the abi3 limited API targets py39, but pyo3 0.28's build script inspects the *running* interpreter (3.14). If it errors that 3.14 is newer than it knows, set `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` before `maturin develop` (abi3 stays compatible across newer CPython). Verify the env-var path works in this task; if pyo3 0.28 already recognizes 3.14, no action needed.

**Step 4: Commit**
`git add crates/autoagent-bingen/pyproject.toml crates/autoagent-bingen/tests_py && git commit -m "feat(bingen): pyo3 abi3 build + python smoke (FR-3)"`

**B2 exit criteria:** `import autoagent; autoagent.doctor(root)` works from a maturin-built module; `.pyi` present; drift clean.

---

## Milestone B3 — Mutating surface + safety parity (FR-5, FR-6, FR-7, FR-20)

### Task B3-T1: `CallbackGate` — bridge core's `ApprovalGate` to a host callback (fail-closed)

**Files:**
- Modify: `crates/autoagent-bingen/bind.rs`
- Test: `crates/autoagent-bingen/tests/approval.rs`

**Step 1: Write failing tests** — `tests/approval.rs` (fail-closed semantics, FR-7/FR-20)
```rust
use autoagent_bingen::bind::{CallbackGate, ApprovalDecision};
use autoagent_core::safety::approval_gate::ApprovalGate;

#[test]
fn no_callback_denies_write() {
    let gate = CallbackGate::deny_all();
    assert!(gate.confirm_write("crates/x.rs").is_err());
}
#[test]
fn callback_false_denies() {
    let gate = CallbackGate::from_fn(|_req| false);
    assert!(gate.confirm_command("cargo test").is_err());
}
#[test]
fn callback_true_allows() {
    let gate = CallbackGate::from_fn(|_req| true);
    assert!(gate.confirm_write("crates/x.rs").is_ok());
    assert!(gate.confirm_command("cargo test").is_ok());
}
#[test]
fn approve_flag_allows_without_callback() {
    let gate = CallbackGate::approve_all();
    assert!(gate.confirm_write("x").is_ok());
}
```

**Step 2: Run to verify failure**
`cargo test -p autoagent-bingen --test approval` → Expected: FAIL

**Step 3: Implement `CallbackGate`** in `bind.rs`
```rust
use autoagent_core::error::PolicyError;
use autoagent_core::safety::approval_gate::ApprovalGate;

#[derive(Clone)]
pub struct ApprovalRequest { pub kind: String, pub target: String }
pub enum ApprovalDecision { Allow, Deny }

/// Bridges core's ApprovalGate to a host callback. Default-on-absence = DENY.
pub struct CallbackGate {
    cb: Box<dyn Fn(ApprovalRequest) -> bool + Send + Sync>,
}
impl CallbackGate {
    pub fn from_fn(f: impl Fn(ApprovalRequest) -> bool + Send + Sync + 'static) -> Self {
        Self { cb: Box::new(f) }
    }
    pub fn approve_all() -> Self { Self::from_fn(|_| true) }
    pub fn deny_all() -> Self { Self::from_fn(|_| false) }
}
impl ApprovalGate for CallbackGate {
    fn confirm_write(&self, target: &str) -> autoagent_core::error::Result<()> {
        if (self.cb)(ApprovalRequest { kind: "write".into(), target: target.into() }) { Ok(()) }
        else { Err(PolicyError::WriteNotApproved(target.into()).into()) }
    }
    fn confirm_command(&self, command: &str) -> autoagent_core::error::Result<()> {
        if (self.cb)(ApprovalRequest { kind: "command".into(), target: command.into() }) { Ok(()) }
        else { Err(PolicyError::CommandNotApproved(command.into()).into()) }
    }
}
```

**Step 4: Run to verify pass**
`cargo test -p autoagent-bingen --test approval` → Expected: PASS

**Step 5: Commit**
`git add crates/autoagent-bingen/bind.rs crates/autoagent-bingen/tests/approval.rs && git commit -m "feat(bingen): CallbackGate bridges ApprovalGate, fail-closed (FR-7/FR-20)"`

---

### Task B3-T2: Mutating wrappers — apply/revert + run/evolve (sync `_from` paths + async)

**Files:**
- Modify: `crates/autoagent-bingen/bind.rs`
- Test: `crates/autoagent-bingen/tests/mutating.rs`

**Step 1: Write failing test** — `tests/mutating.rs` (uses a real plan + temp workspace; no LLM → `from` path)
```rust
use autoagent_bingen::bind;

#[test]
fn apply_with_approval_then_revert_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    bind::init(root).unwrap();
    // Minimal valid plan that creates one file (schema_version=1).
    let plan = root_plan(root);
    let run_id = bind::apply(root, &plan, /*approve=*/true).unwrap();
    assert!(!run_id.is_empty());
    bind::revert(root, &run_id).unwrap();
}
#[test]
fn apply_without_approval_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    bind::init(root).unwrap();
    let plan = root_plan(root);
    let err = bind::apply(root, &plan, /*approve=*/false).unwrap_err();
    assert!(err.code.starts_with("policy"));
}
// helper writes a tiny plan json file, returns its path (impl in test)
```
> Implementation note: `root_plan` writes a `*.plan.json` with `schema_version:1` and one `create_file` op into `.agent/plans/`, matching `plan_schema::plan_json_schema()`. Read that schema in this task to get exact field names — do NOT guess.

**Step 2: Run to verify failure**
`cargo test -p autoagent-bingen --test mutating` → Expected: FAIL

**Step 3: Implement mutating wrappers** in `bind.rs` (gate-aware; async via tokio block_on for `_sync`)
```rust
pub fn apply(root: &str, plan_path: &str, approve: bool) -> BindResult {
    let root = utf8(root)?;
    let gate = if approve { CallbackGate::approve_all() } else { CallbackGate::deny_all() };
    let run_id = autoagent_core::runtime::agent_loop::apply_with_gate(
        root, utf8(plan_path)?, &gate)?;
    Ok(run_id)
}
pub fn revert(root: &str, run_id: &str) -> BindResult {
    autoagent_core::runtime::revert::revert(utf8(root)?, run_id)?;
    Ok(String::new())
}
/// Sync run from an existing plan (no LLM). Mirrors commands::run --from path.
pub fn run_from_sync(root: &str, plan_path: &str, approve: bool) -> BindResult {
    let gate = if approve { CallbackGate::approve_all() } else { CallbackGate::deny_all() };
    // run_with_plan already applies+validates; gate decision pre-checked here.
    gate.confirm_write("planned changes").map_err(BindError::from)?;
    let outcome = autoagent_core::runtime::run_workflow::run_with_plan(utf8(root)?, utf8(plan_path)?, true)?;
    Ok(serde_json::to_string(&outcome).map_err(serde_err)?)
}
pub fn evolve_from_sync(root: &str, objective: &str, plan_path: &str, apply: bool) -> BindResult {
    let plan = autoagent_core::planning::plan_reader::read_plan(utf8(plan_path)?)?;
    let outcome = autoagent_core::runtime::evolve_workflow::evolve_with_plan(utf8(root)?, objective, &plan, apply)?;
    Ok(serde_json::to_string(&outcome).map_err(serde_err)?)
}
```
> The LLM-generating async variants (`run_workflow`, `evolve_generated`) need a provider; expose `run_generate`/`evolve_generate` that build the provider exactly as `commands::run`/`evolve` do (read `commands/mod.rs:37-46,93-102` for the precise `build_provider`/`tokio::Runtime` pattern) and wrap with the gate. Add them in this step too.

**Step 4: Run to verify pass**
`cargo test -p autoagent-bingen --test mutating` → Expected: PASS

**Step 5: Commit**
`git add crates/autoagent-bingen/bind.rs crates/autoagent-bingen/tests/mutating.rs && git commit -m "feat(bingen): mutating wrappers apply/revert/run/evolve via gate (FR-5/FR-6)"`

---

### Task B3-T3: Generator — emit mutating + async fns for napi & pyo3 (Promise / async + `_sync`)

**Files:**
- Modify: `crates/autoagent-bingen/src/gen/emit.rs` (`napi_fn`, `pyo3_fn`)
- Test: `crates/autoagent-bingen/tests/generate.rs` (extend)

**Step 1: Write failing test**
```rust
#[test]
fn generate_emits_async_and_mutating() {
    let out = autoagent_bingen::gen::render_all();
    let napi = out.get("src/node/napi.rs").unwrap();
    assert!(napi.contains("pub fn apply"));
    assert!(napi.contains("AsyncTask") || napi.contains("async fn run")); // Promise bridge
    let py = out.get("src/python/pyrs.rs").unwrap();
    assert!(py.contains("fn apply"));
    assert!(py.contains("run_sync"));
}
```

**Step 2: Run → FAIL**
`cargo test -p autoagent-bingen --test generate` → Expected: FAIL

**Step 3: Extend the emitters** — add `apply`/`revert`/`run`/`evolve` arms to `napi_fn`/`pyo3_fn`. napi async via `#[napi] pub async fn run(...)` (napi-rs 3.x `tokio_rt`); pyo3 async via **`pyo3-async-runtimes`** (`future_into_py`, tokio runtime) plus a `*_sync` variant calling the `block_on` wrapper. (`pyo3-async-runtimes 0.28` is already declared under the `py-pyo3` feature in B1-T1 — NOT the dead `pyo3-asyncio`.)

**Step 4: Run → PASS + regenerate + drift check**
`cargo test -p autoagent-bingen --test generate && cargo run -p autoagent-bingen -- generate && cargo test -p autoagent-bingen --test drift` → Expected: PASS

**Step 5: Commit**
`git add crates/autoagent-bingen && git commit -m "feat(bingen): generate mutating + async (Promise/async+_sync) for napi & pyo3 (FR-5)"`

---

### Task B3-T4: Fail-closed approval tests through the actual backends (Node + Python)

**Files:**
- Create: `crates/autoagent-bingen/__test__/approval.test.mjs`
- Create: `crates/autoagent-bingen/tests_py/test_approval.py`

**Step 1: Write failing tests** — privileged op with no approval must throw a `policy.*` error in each runtime
```javascript
// __test__/approval.test.mjs (node:test)
import test from "node:test"; import assert from "node:assert";
const addon = /* load built addon (B5 loader) */;
test("apply without approval throws policy error", () => {
  assert.throws(() => addon.apply(process.cwd(), "missing.plan.json", { approve: false }),
    /policy|GenericFailure/);
});
```
```python
# tests_py/test_approval.py
import autoagent, pytest
def test_apply_without_approval_refused(tmp_path):
    autoagent.init(str(tmp_path))
    with pytest.raises(Exception) as e:
        autoagent.apply(str(tmp_path), "missing.plan.json", approve=False)
    assert "policy" in str(e.value).lower() or "approv" in str(e.value).lower()
```

**Step 2: Build backends + run** → Expected: FAIL first (until generated mutating fns are built in)
`cargo run -p autoagent-bingen -- generate && maturin develop --features py-pyo3 && pytest tests_py/test_approval.py -q` then the node test via `node --test __test__/`

**Step 3: Make pass** — ensure the generated mutating fns map `BindError`→host error with the `policy.*` code preserved (already via `to_napi`/`to_py`). Adjust if the error string lost the code.

**Step 4: Commit**
`git add crates/autoagent-bingen/__test__ crates/autoagent-bingen/tests_py && git commit -m "test(bingen): fail-closed approval through napi & pyo3 backends (FR-7/FR-20)"`

---

### Task B3-T5: Safety-parity E2E — binding `run` trail == CLI `run` trail

**Files:**
- Create: `crates/autoagent-bingen/tests/parity_e2e.rs`

**Step 1: Write the E2E test** (real workspace, real `Autoagent.toml`, no mocked core; client→binding→core→fs)
```rust
//! E2E: a run applied via the binding produces the same .agent/ artifacts as
//! the CLI for the same plan, and is revertible. No mocked layers.
use std::process::Command;

#[test]
fn binding_run_matches_cli_run_trail() {
    // 1. Two sibling temp workspaces seeded identically.
    let bdir = tempfile::tempdir().unwrap();
    let cdir = tempfile::tempdir().unwrap();
    seed_workspace(bdir.path());   // init + identical source + identical plan file
    seed_workspace(cdir.path());

    // 2. Binding path: apply via bind::apply (in-process, real core).
    let plan_b = format!("{}/.agent/plans/p.plan.json", bdir.path().display());
    let run_b = autoagent_bingen::bind::apply(bdir.path().to_str().unwrap(), &plan_b, true).unwrap();

    // 3. CLI path: run the real `autoagent` binary on the twin workspace.
    let plan_c = format!("{}/.agent/plans/p.plan.json", cdir.path().display());
    let out = Command::new(env!("CARGO_BIN_EXE_autoagent")) // requires cli as dev-dep or path
        .args(["--yes", "apply", &plan_c]).current_dir(cdir.path()).output().unwrap();
    assert!(out.status.success(), "cli apply failed: {}", String::from_utf8_lossy(&out.stderr));

    // 4. Compare the resulting patch bodies (normalize run-id/timestamps).
    let patch_b = read_patch(bdir.path(), &run_b);
    let patch_c = read_only_patch(cdir.path());
    assert_eq!(normalize(&patch_b), normalize(&patch_c), "binding vs CLI patch diverged");

    // 5. Revert via binding restores the workspace.
    autoagent_bingen::bind::revert(bdir.path().to_str().unwrap(), &run_b).unwrap();
    assert!(workspace_clean(bdir.path()));
}
```
> Helpers (`seed_workspace`, `read_patch`, `normalize`, `workspace_clean`) implemented in this file. `CARGO_BIN_EXE_autoagent` requires adding `autoagent-cli` as a dev-dependency OR invoking the workspace-built binary by path — read the M8 CI workflow for how the binary is located, do NOT hardcode a target path blindly.

**Step 2: Run**
`cargo test -p autoagent-bingen --test parity_e2e -- --nocapture` → Expected: PASS

**Step 3: Commit**
`git add crates/autoagent-bingen/tests/parity_e2e.rs && git commit -m "test(bingen): safety-parity E2E — binding run trail == CLI (FR-6, no-bypass)"`

**B3 exit criteria:** mutating ops route through `ApprovalGate`; no-approval refuses in Node & Python; parity E2E green; revert via binding works.

---

## Milestone B4 — Deno + secondary backends (FR-3 all six)

### Task B4-T1: Generator — emit raw FFI backend (`deno/ffi.rs`) + C-ABI string/free glue

**Files:**
- Modify: `crates/autoagent-bingen/src/gen/emit.rs`
- Modify: `crates/autoagent-bingen/bind.rs` (add `cstring`/`free` helpers)
- Test: `crates/autoagent-bingen/tests/generate.rs`

**Step 1: Write failing test**
```rust
#[test]
fn generate_emits_ffi_with_cabi() {
    let out = autoagent_bingen::gen::render_all();
    let ffi = out.get("src/deno/ffi.rs").unwrap();
    assert!(ffi.contains("#[no_mangle]"));
    assert!(ffi.contains("extern \"C\""));
    assert!(ffi.contains("pub extern \"C\" fn aa_free"));
}
```

**Step 2: Run → FAIL**

**Step 3: Implement** — `ffi.rs` emitter: each Read symbol becomes `#[no_mangle] pub extern "C" fn aa_<name>(root: *const c_char) -> *mut c_char` returning a JSON `CString::into_raw`, plus a single `aa_free(ptr)`. Add the `cstr_in`/`cstr_out`/`aa_free` helpers to `bind.rs` (the `(ptr,len)` JSON-string ABI, D-9). Mutating fns take extra `*const c_char` args; errors encode as `{"__error__": {...}}` JSON so the TS side can throw.

**Step 4: Run → PASS + regenerate + drift**

**Step 5: Commit**
`git commit -m "feat(bingen): generate raw C-ABI FFI backend for Deno (FR-3)"`

---

### Task B4-T2: Generator — emit `deno_bindgen` backend + `mod.ts` TS FFI wrapper

**Files:**
- Modify: `crates/autoagent-bingen/src/gen/emit.rs`
- Test: `crates/autoagent-bingen/tests/generate.rs`; `crates/autoagent-bingen/deno/smoke.ts`

**Step 1: Write failing test**
```rust
#[test]
fn generate_emits_deno_bindgen_and_modts() {
    let out = autoagent_bingen::gen::render_all();
    assert!(out.get("src/deno/deno_bindgen.rs").unwrap().contains("#[deno_bindgen]"));
    let modts = out.get("deno/mod.ts").unwrap();
    assert!(modts.contains("Deno.dlopen"));
    assert!(modts.contains("export function doctor"));
}
```

**Step 2: Run → FAIL**

**Step 3: Implement** — `deno_bindgen.rs` emitter (annotate wrappers with `#[deno_bindgen]`) and a `mod.ts` generator that emits the typed `Deno.dlopen` symbol table + per-symbol wrappers reusing the shared `.d.ts` types, hiding `(ptr,len)`+`aa_free` (async ops use `{ nonblocking: true }`, R-10; approval via `Deno.UnsafeCallback`).

**Step 4: Run → PASS + regenerate + drift**

**Step 5: Deno smoke** — `crates/autoagent-bingen/deno/smoke.ts` imports `./mod.ts`, calls `doctor(Deno.cwd())`. Run:
`cargo build -p autoagent-bingen --features deno-ffi --release && deno run --allow-ffi --allow-read crates/autoagent-bingen/deno/smoke.ts` → Expected: prints check count.

**Step 6: Commit**
`git commit -m "feat(bingen): generate deno_bindgen backend + mod.ts FFI wrapper (FR-3)"`

---

### Task B4-T3: Generator — emit node-bindgen + rustpython backends

**Files:**
- Modify: `crates/autoagent-bingen/src/gen/emit.rs`
- Test: `crates/autoagent-bingen/tests/generate.rs`

**Step 1: Write failing test** — assert `src/node/node_bindgen.rs` contains `#[node_bindgen]` and `src/python/python_bingen.rs` contains a RustPython module registration.

**Step 2: Run → FAIL**

**Step 3: Implement** both emitters. node-bindgen: `#[node_bindgen]` per symbol over the neutral wrappers. rustpython: expose the surface as a RustPython native module (per Q-1 resolution — if RustPython embedding proves unviable in this task, gate `python_bingen.rs` behind an off-by-default feature and emit a clearly-marked `unimplemented!()`-free minimal `version()`/`doctor()` module, recording the limitation in the spec's Q-1; do NOT ship a fake surface).

**Step 4: Run → PASS + regenerate + drift**

**Step 5: Build-check both** — `cargo build -p autoagent-bingen --features node-bindgen` and `--features py-rustpython` compile.

**Step 6: Commit**
`git commit -m "feat(bingen): generate node-bindgen + rustpython backends (FR-3)"`

---

### Task B4-T4: Cross-backend equivalence + all-six smoke

**Files:**
- Modify: `crates/autoagent-bingen/src/gen/mod.rs` (`smoke` runs all available backends)
- Create: `crates/autoagent-bingen/tests/equivalence.rs`

**Step 1: Write test** — `doctor()` JSON from the neutral wrapper is byte-equal regardless of backend (they all call `bind::doctor`), and a Deno `doctor()` parsed result equals a Node one (shape check via a script the test shells out to).

**Step 2: Run → PASS** (wrappers are shared, so equivalence holds by construction; the test documents+guards it).

**Step 3: Extend `smoke()`** to iterate every compiled-in backend feature and run its loader, reporting per-backend OK.

**Step 4: Commit**
`git commit -m "test(bingen): cross-backend equivalence + six-backend smoke (FR-3)"`

**B4 exit criteria:** all six backends build behind features; `bingen smoke` exercises each; Deno `doctor()` == Node `doctor()`; drift clean.

---

## Milestone B5 — Distribution + CI matrix (FR-13, FR-14, FR-10)

### Task B5-T1: Generator — emit package scaffolds (package.json, pyproject.toml, deno.json, loaders)

**Files:**
- Modify: `crates/autoagent-bingen/src/gen/emit.rs`
- Test: `crates/autoagent-bingen/tests/generate.rs`

**Step 1: Write failing test** — assert `render_all()` includes `package.json` (with `@autoagent/native` + napi config), `dist/index.js` loader, `pyproject.toml`, `deno.json`, and `python/autoagent/__init__.py` loader.

**Step 2: Run → FAIL**

**Step 3: Implement** the scaffold emitters (napi triples + `main`/`types`; maturin config already in `pyproject.toml` from B2 — generate-or-verify; `deno.json` with `exports` → `mod.ts`; loader shims that resolve the platform binary). Reconcile with the hand-written `pyproject.toml`/`package.json` from earlier tasks so generation is authoritative (FR-10).

**Step 4: Run → PASS + regenerate + drift**

**Step 5: Commit**
`git commit -m "feat(bingen): generate package scaffolds for npm/PyPI/JSR (FR-10)"`

---

### Task B5-T2: napi loader shim → real `.node` resolution; fix smoke loaders

**Files:**
- Modify: `crates/autoagent-bingen/__test__/smoke.mjs`, `deno/smoke.ts`
- Modify: generated `dist/index.js`

**Step 1:** Replace the raw-cdylib lookup (B1-T8 TODO) with the generated napi loader that resolves `autoagent.<platform>.node`. Use `napi-cli` (`napi build --platform`) to produce the `.node` artifact name.

**Step 2: Run** `napi build --platform --release` (added as an npm script) then `node --test __test__/` → Expected: PASS.

**Step 3: Commit**
`git commit -m "fix(bingen): real .node loader resolution; close B1-T8 smoke TODO"`

---

### Task B5-T3: CI matrix — build/test/`check` across 6 cells

**Files:**
- Create: `.github/workflows/bingen.yml`

**Step 1: Write the workflow** — matrix `os: [ubuntu, macos, windows] × arch: [x64, arm64]`; jobs: (a) `cargo test -p autoagent-bingen` (all unit/integration incl. parity E2E on native), (b) `bingen check` drift gate, (c) napi build + `node --test`, (d) `maturin build` + `pytest`, (e) `deno test --allow-ffi`. Mirror the existing M8 release workflow conventions (read `.github/workflows/` first).

**Step 2: Validate locally** — `act` or push a branch; ensure the drift gate and parity E2E run. (If `act`/runners unavailable locally, validate YAML with `yamllint` and the documented job graph; note in commit that cloud CI is the real gate.)

**Step 3: Commit**
`git commit -m "build(bingen): CI matrix — 6 cells, drift gate + parity E2E (FR-14)"`

---

### Task B5-T4: Publish workflow — npm + PyPI + JSR (tagged release)

**Files:**
- Create: `.github/workflows/bingen-release.yml`

**Step 1: Write** the release workflow: on tag, build the prebuild matrix, `npm publish @autoagent/native` (with platform `.node` optionalDependencies), `twine upload` abi3 wheels, `deno publish` to JSR + attach the `cdylib` as a release asset the loader fetches (Q-8). Publish gated on `bingen check` + parity E2E green.

**Step 2: Dry-run** — `npm publish --dry-run`, `twine check dist/*`, `deno publish --dry-run` in the workflow.

**Step 3: Commit**
`git commit -m "build(bingen): release workflow publishes npm/PyPI/JSR prebuilds (FR-13)"`

**B5 exit criteria:** all 6 cells build+test+install; `npm install`, `pip install`, `deno run` load and `doctor()` on each cell; `bingen check` gates release.

---

## Final verification (run before declaring the plan complete)

1. `cargo test --workspace` → all green (core + bingen, incl. parity E2E).
2. `cargo run -p autoagent-bingen -- generate && cargo run -p autoagent-bingen -- check` → no drift.
3. `cargo run -p autoagent-bingen -- smoke` → all compiled backends load and call `doctor()`.
4. Per-runtime smoke: `node --test`, `pytest`, `deno test --allow-ffi` → green.
5. Spec traceability: confirm FR-1..FR-21 each map to a task above (FR-17/18 are COULD — explicitly deferred; note as TODO, do not implement).

## Deferred (noted, NOT in scope of this plan)
- FR-17 (extra `.proto`/OpenAPI stub flavor) and FR-18 (progress streaming/cancellation) — COULD-priority; leave as TODOs.
- Native object marshaling beyond JSON-at-boundary (Q-4) — optimization, not built here.
- RustPython full surface if Q-1 resolves negative — keep feature off-by-default with the limitation recorded in the spec.
