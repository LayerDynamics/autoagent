# AutoAgent Language SDKs (Python · Node/TS · Deno) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use lore:execute to implement this plan task-by-task.
> **Scope guard:** Do ONLY what is listed here. If you discover adjacent issues, note them as a TODO and continue. Do NOT fix them.

**Goal:** Build ergonomic, idiomatic, typed SDKs in Python, Node.js/TypeScript, and Deno that wrap the existing `autoagent-bingen` native bindings — turning the low-level surface (JSON strings in Python, `serde_json::Value` in Node, parsed objects in Deno) into published libraries with typed models, error classes, and both a functional API and a client class.

**Architecture:** Core's boundary structs gain additive `#[derive(schemars::JsonSchema)]`. The `bingen` generator emits one `models.schema.json` (JSON Schema of every boundary model) and, from it, per-language **model files** (Python `@dataclass`, TS `interface`) into each SDK tree — drift-guarded by `bingen check`. On top of those generated models sit **hand-written** SDK layers in `sdk/python`, `sdk/node`, `sdk/deno`: a typed functional API (parse the native output → typed model, map errors → `AutoAgentError`) and a thin `AutoAgent(root)` client class. Each SDK depends on the native binding (`@autoagent/native` / the pyo3 extension / the Deno FFI cdylib).

**Tech Stack:** Rust + `schemars` 1.2 (core + bingen); Python 3.9+ (`@dataclass`, maturin mixed Rust/Python layout, `py.typed`); TypeScript 5 + tsup/tsc (npm) + Node ≥18; Deno 2.x (JSR). Existing native bindings from `crates/autoagent-bingen` (napi-rs, pyo3, deno_bindgen/FFI).

**Practices:** **Typed-first** (generate models + define the public types before wrapper logic) → **TDD** (each SDK method: failing test asserting typed/ergonomic behavior + a real round-trip against the native binding, then implement) → **Drift-guarded codegen** (every generated model file is covered by `bingen check`).

**Required skills:** none (Rust + Python + TS/Deno SDK packaging; not a Claude Code extension).

**Builds on:** [SPEC-2: autoagent-bingen](../specs/SPEC-2-autoagent-bingen.md) (the native bindings) and [`docs/plans/2026-06-09-autoagent-bingen.md`](./2026-06-09-autoagent-bingen.md).

---

## Ground truth (verified — do NOT re-derive)

- **Native binding return shapes today:** Python pyo3 returns **JSON strings** (`doctor(root) -> str`); Node napi returns **objects** (`serde_json::Value`, typed via `ts_return_type`); Deno `mod.ts` returns **parsed objects**, `bindings.ts` returns **status-tagged strings**. Async `run`/`evolve` exist (napi Promise, pyo3 awaitable, deno_bindgen non_blocking).
- **Boundary model structs (core, all `serde`):** `DoctorReport`/`Check` (`runtime/doctor.rs`), `RunOutcome`/`RunState`/`ValidationReport`/`CommandValidationResult` (`runtime/run_workflow.rs`, `runtime/run_state.rs`, `validation/validation_report.rs`), `EvolveOutcome` (`runtime/evolve_workflow.rs`), `ProjectAnalysis`/`LanguageKind`/`PackageManager`/`DependencySummary` (`analysis/project_analysis.rs`), `Plan`/`PlannedFile` (`planning/plan.rs`).
- **`MemorySummary` is NOT a struct** — `bind.rs:245` builds it ad-hoc via `serde_json::json!`. It must be promoted to a typed struct (S1-T3) so the model schema covers it.
- **Error shape:** `bind::BindError { code: String, exit_code: i32, message: String }`; native errors carry `[code|exit_code] message`. Python pyo3 raises `AutoAgentError`; the tagged-string backends prefix `1` + JSON error.
- **Registry return types in use:** DoctorReport, RunOutcome (×2), EvolveOutcome (×2), ProjectAnalysis, MemorySummary, string, string[], number, boolean, void.
- schemars latest stable: **1.2.1**.

---

## Milestone S1 — Model schema foundation + per-language model codegen

### Task S1-T1: Additive `schemars::JsonSchema` derives on core boundary structs

**Files (modify each `#[derive(...)]`):**
- `crates/autoagent-core/src/runtime/doctor.rs` (`Check`, `DoctorReport`)
- `crates/autoagent-core/src/runtime/run_state.rs` (`RunState`)
- `crates/autoagent-core/src/runtime/run_workflow.rs` (`RunOutcome`)
- `crates/autoagent-core/src/validation/validation_report.rs` (`ValidationReport`, `CommandValidationResult`)
- `crates/autoagent-core/src/runtime/evolve_workflow.rs` (`EvolveOutcome`)
- `crates/autoagent-core/src/analysis/project_analysis.rs` (`ProjectAnalysis`, `LanguageKind`, `PackageManager`, `DependencySummary`)
- `crates/autoagent-core/src/planning/plan.rs` (`Plan`, `PlannedFile`)
- `crates/autoagent-core/Cargo.toml` (add `schemars = "1"` to `[workspace.dependencies]` + use here)

**Step 1: Write the failing test** — `crates/autoagent-core/src/analysis/project_analysis.rs` tests module:
```rust
#[test]
fn project_analysis_has_json_schema() {
    let schema = schemars::schema_for!(ProjectAnalysis);
    let json = serde_json::to_string(&schema).unwrap();
    assert!(json.contains("source_files"));
}
```

**Step 2: Run to verify it fails**
`cargo test -p autoagent-core project_analysis_has_json_schema` → Expected: FAIL (no `JsonSchema`)

**Step 3: Add `schemars` workspace dep + additive derives**
In root `Cargo.toml` `[workspace.dependencies]`: `schemars = "1"`. In `crates/autoagent-core/Cargo.toml`: `schemars.workspace = true`. Add `schemars::JsonSchema` to each struct/enum's derive list, e.g.:
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DoctorReport { /* unchanged */ }
```
(Behavior-preserving; no field changes. `Utf8PathBuf` in `EvolveOutcome`/`ProjectAnalysis`/`Plan` — `camino` implements `JsonSchema` under its `schemars1` feature; enable it: in root `Cargo.toml` set `camino = { version = "1", features = ["serde1", "schemars1"] }`.)

**Step 4: Run to verify pass + no regressions**
`cargo test -p autoagent-core` → Expected: PASS

**Step 5: Commit**
`git add crates/autoagent-core Cargo.toml Cargo.lock && git commit -m "feat(core): additive JsonSchema derives on boundary structs (SDK models)"`

---

### Task S1-T2: Promote `MemorySummary` to a typed struct

**Files:**
- Create: `crates/autoagent-core/src/memory/summary.rs` (`MemorySummary` struct)
- Modify: `crates/autoagent-core/src/memory/mod.rs` (declare `pub mod summary;`)
- Modify: `crates/autoagent-bingen/bind.rs` (`memory_show` builds + serializes the struct)

**Step 1: Write the struct** (`summary.rs`)
```rust
//! Typed project-memory summary returned by the `memory` binding (was an
//! ad-hoc serde_json::json! in bingen).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MemorySummary {
    pub name: String,
    pub language: String,
    pub package_manager: Option<String>,
    pub source_file_count: usize,
    pub decisions: usize,
}
```

**Step 2: Wire it in `bind.rs::memory_show`** — replace the `serde_json::json!({...})` block with:
```rust
let summary = autoagent_core::memory::summary::MemorySummary {
    name: pm.name,
    language: pm.language,
    package_manager: pm.package_manager,
    source_file_count: pm.source_file_count,
    decisions: decisions.len(),
};
Ok(serde_json::to_string(&summary).map_err(serde_err)?)
```

**Step 3: Verify** `cargo test -p autoagent-bingen --test wrappers` (the `memory_show_after_init_reports_project` test still passes) → Expected: PASS

**Step 4: Commit**
`git add crates/autoagent-core/src/memory crates/autoagent-bingen/bind.rs && git commit -m "refactor(core): typed MemorySummary struct (replaces ad-hoc json)"`

---

### Task S1-T3: bingen emits `models.schema.json` from the core structs

**Files:**
- Modify: `crates/autoagent-bingen/Cargo.toml` (add `schemars.workspace = true`)
- Modify: `crates/autoagent-bingen/src/gen/emit.rs` (`models_schema()` + register in `render_all`)
- Test: `crates/autoagent-bingen/tests/generate.rs`

**Step 1: Write the failing test**
```rust
#[test]
fn models_schema_emitted() {
    let out = gen::render_all();
    let s = out.get("schema/models.schema.json").expect("models schema");
    for ty in ["DoctorReport", "RunOutcome", "EvolveOutcome", "ProjectAnalysis", "MemorySummary"] {
        assert!(s.contains(ty), "models schema missing {ty}");
    }
}
```

**Step 2: Run → FAIL**

**Step 3: Implement `models_schema()`** in `emit.rs` — aggregate `schemars::schema_for!` for each boundary type into one `$defs` document:
```rust
fn models_schema() -> String {
    use schemars::schema_for;
    let mut defs = serde_json::Map::new();
    macro_rules! add { ($t:ty, $name:expr) => {
        defs.insert($name.into(), serde_json::to_value(schema_for!($t)).unwrap());
    }; }
    add!(autoagent_core::runtime::doctor::DoctorReport, "DoctorReport");
    add!(autoagent_core::runtime::run_workflow::RunOutcome, "RunOutcome");
    add!(autoagent_core::runtime::evolve_workflow::EvolveOutcome, "EvolveOutcome");
    add!(autoagent_core::analysis::project_analysis::ProjectAnalysis, "ProjectAnalysis");
    add!(autoagent_core::memory::summary::MemorySummary, "MemorySummary");
    let doc = serde_json::json!({ "version": "1.0.0", "models": defs });
    serde_json::to_string_pretty(&doc).unwrap() + "\n"
}
```
Register: `m.insert("schema/models.schema.json".into(), models_schema());`

**Step 4: Run → PASS + regenerate + drift**
`cargo test -p autoagent-bingen --test generate && cargo run -p autoagent-bingen -- generate && cargo run -p autoagent-bingen -- check` → PASS

**Step 5: Commit**
`git add crates/autoagent-bingen && git commit -m "feat(bingen): emit models.schema.json from core JsonSchema (S1-T3)"`

---

### Task S1-T4: Generate per-language model files (Python dataclasses, TS interfaces, Deno)

**Files:**
- Modify: `crates/autoagent-bingen/src/gen/emit.rs` (`py_models()`, `ts_models()` + register, writing into the SDK trees)
- Modify: `crates/autoagent-bingen/src/gen/mod.rs` (allow writing to `../../sdk/...` repo-relative paths)
- Test: `crates/autoagent-bingen/tests/generate.rs`

**Step 1: Write the failing test** — assert `render_all()` contains `sdk/python/autoagent/_models.py` with `@dataclass\nclass DoctorReport` and `sdk/node/src/_models.ts` with `export interface RunOutcome`.

**Step 2: Run → FAIL**

**Step 3: Implement `py_models()` + `ts_models()`** — walk the `models.schema.json` `$defs`, emit:
- Python: `from dataclasses import dataclass` + one `@dataclass` per model (fields typed from the schema: `str`/`int`/`bool`/`Optional[...]`/`list[...]`/nested model), plus a `from_dict` classmethod.
- TypeScript: one `export interface` per model.
Register both into `render_all()` writing to `../../sdk/python/autoagent/_models.py` and `../../sdk/node/src/_models.ts` and `../../sdk/deno/_models.ts` (Deno re-uses the TS interfaces). Generated files carry the "DO NOT EDIT" header.
> NOTE: `gen/mod.rs` joins paths under `CARGO_MANIFEST_DIR`; `../../sdk/...` resolves to repo-root `sdk/`. Confirm the `bingen check` drift guard reads them back from the same path.

**Step 4: Run → PASS + regenerate + drift clean**

**Step 5: Commit**
`git add crates/autoagent-bingen sdk && git commit -m "feat(bingen): generate per-language SDK model files from schema (S1-T4)"`

**S1 exit criteria:** `bingen generate` emits `models.schema.json` + `sdk/python/autoagent/_models.py` + `sdk/node/src/_models.ts` + `sdk/deno/_models.ts`; `bingen check` clean; core tests green.

---

## Milestone S2 — Python SDK (`sdk/python`)

### Task S2-T1: Repackage the native extension as `autoagent._native`

**Files:**
- Create: `sdk/python/pyproject.toml` (maturin mixed layout: `python-source = "."`, module-name `autoagent._native`)
- Modify: `crates/autoagent-bingen/src/python/pyrs.rs` generation — the `#[pymodule]` stays `autoagent` at the Rust level but maturin installs it as the `_native` submodule of the `autoagent` package.
- Create: `sdk/python/autoagent/__init__.py` (placeholder re-export, fleshed out S2-T3)

**Step 1: Write `sdk/python/pyproject.toml`**
```toml
[build-system]
requires = ["maturin>=1.5,<2"]
build-backend = "maturin"

[project]
name = "autoagent"
requires-python = ">=3.9"
license = { text = "MIT" }
dynamic = ["version"]

[tool.maturin]
manifest-path = "../../crates/autoagent-bingen/Cargo.toml"
features = ["py-pyo3"]
module-name = "autoagent._native"
python-source = "."
```

**Step 2: Build + verify the native is importable as `autoagent._native`**
`cd sdk/python && python -m venv .venv && . .venv/bin/activate && pip install maturin pytest && maturin develop`
`python -c "from autoagent import _native; print(_native.version())"` → Expected: `1`

**Step 3: Commit**
`git add sdk/python && git commit -m "build(sdk-py): package native extension as autoagent._native (S2-T1)"`

---

### Task S2-T2: `AutoAgentError` + typed models wiring

**Files:**
- Create: `sdk/python/autoagent/errors.py` (`AutoAgentError(Exception)` with `code`, `exit_code`)
- The generated `sdk/python/autoagent/_models.py` (from S1-T4) is consumed here.
- Create: `sdk/python/autoagent/py.typed` (PEP 561 marker)
- Test: `sdk/python/tests/test_models.py`

**Step 1: Write the failing test**
```python
import json
from autoagent._models import DoctorReport
def test_doctor_report_from_dict():
    d = DoctorReport.from_dict(json.loads('{"checks":[{"name":"x","ok":true,"detail":"d"}]}'))
    assert d.checks[0].name == "x" and d.checks[0].ok is True
```

**Step 2: Run → FAIL** (no model / from_dict)

**Step 3: Implement** `errors.py` (`AutoAgentError`) and confirm the generated `_models.py` has a working `from_dict`. If S1-T4's `from_dict` is incomplete for nested lists, fix the generator (drift-checked).

**Step 4: Run → PASS**

**Step 5: Commit**
`git add sdk/python && git commit -m "feat(sdk-py): AutoAgentError + typed model parsing (S2-T2)"`

---

### Task S2-T3: Functional API — parse native JSON → models, raise `AutoAgentError`

**Files:**
- Modify: `sdk/python/autoagent/__init__.py`
- Test: `sdk/python/tests/test_functional.py`

**Step 1: Write the failing test**
```python
import autoagent
def test_doctor_returns_typed_report(tmp_path):
    report = autoagent.doctor(str(tmp_path))          # returns DoctorReport, not str
    assert hasattr(report, "checks")
def test_apply_without_approval_raises(tmp_path):
    autoagent.init(str(tmp_path))
    import pytest
    with pytest.raises(autoagent.AutoAgentError) as e:
        autoagent.apply(str(tmp_path), "missing.json", approve=False)
    assert e.value.code.startswith("policy")
```

**Step 2: Run → FAIL**

**Step 3: Implement `__init__.py`** — wrap each native function: parse the JSON string into the typed model, map native exceptions → `AutoAgentError` (parse the `[code|exit_code] message` shape), expose async `run`/`evolve` (await the native awaitable, parse). Example:
```python
import json
from . import _native
from .errors import AutoAgentError
from ._models import DoctorReport, RunOutcome, EvolveOutcome, ProjectAnalysis, MemorySummary

def _call(fn, *args):
    try:
        return fn(*args)
    except Exception as e:               # native AutoAgentError
        raise AutoAgentError.from_native(e) from None

def doctor(root: str) -> DoctorReport:
    return DoctorReport.from_dict(json.loads(_call(_native.doctor, root)))

def apply(root: str, plan_path: str, *, approve: bool = False) -> str:
    return _call(_native.apply, root, plan_path, approve)   # returns run id

async def run(root: str, objective: str, from_: str | None = None, *, approve: bool = False) -> RunOutcome:
    out = await _native.run(root, objective, from_, approve)
    return RunOutcome.from_dict(json.loads(out))
# ... version/analyze/init/config_show/patch_list/patch_show/memory_show/tools_list/revert/run_sync/evolve/evolve_sync
```

**Step 4: Run → PASS** (`pytest tests/ -q`)

**Step 5: Commit**
`git add sdk/python && git commit -m "feat(sdk-py): typed functional API over native bindings (S2-T3)"`

---

### Task S2-T4: `AutoAgent` client class + `.pyi` + round-trip safety test

**Files:**
- Create: `sdk/python/autoagent/client.py` (`AutoAgent(root)` holding the root, methods delegate to functional API)
- Modify: `sdk/python/autoagent/__init__.py` (re-export `AutoAgent`)
- Test: `sdk/python/tests/test_client.py`, `sdk/python/tests/test_drift.py`

**Step 1: Write failing tests** — `AutoAgent("/repo").doctor()` returns a `DoctorReport`; `await AutoAgent(root).run(plan, approve=True)` returns a `RunOutcome`; a **drift test** that parses a real `doctor()`/`analyze()` native result into the model and asserts every model field is populated (catches core→model divergence).

**Step 2: Run → FAIL**

**Step 3: Implement `client.py`**
```python
class AutoAgent:
    def __init__(self, root: str): self.root = root
    def doctor(self): return doctor(self.root)
    async def run(self, objective, from_=None, *, approve=False): return await run(self.root, objective, from_, approve=approve)
    def apply(self, plan_path, *, approve=False): return apply(self.root, plan_path, approve=approve)
    # ... full surface
```

**Step 4: Run → PASS**

**Step 5: Commit**
`git add sdk/python && git commit -m "feat(sdk-py): AutoAgent client class + drift safety test (S2-T4)"`

**S2 exit criteria:** `import autoagent; autoagent.doctor(root)` returns a typed `DoctorReport`; `await autoagent.run(...)` returns a `RunOutcome`; fail-closed raises `AutoAgentError`; `AutoAgent(root)` class works; pytest green.

---

## Milestone S3 — Node/TypeScript SDK (`sdk/node`)

### Task S3-T1: Package scaffold depending on `@autoagent/native`

**Files:** Create `sdk/node/package.json` (name `@autoagent/sdk`, dep `@autoagent/native`, `type: module`, tsup build), `sdk/node/tsconfig.json`. The generated `sdk/node/src/_models.ts` (S1-T4) is the model source.

**Step:** `cd sdk/node && npm install && npm run build` produces `dist/`. Commit.

### Task S3-T2: Typed functional API + `AutoAgentError`

**Files:** `sdk/node/src/errors.ts` (`AutoAgentError extends Error` with `code`, `exitCode`), `sdk/node/src/index.ts` (typed wrappers — napi already returns objects, so cast to the `_models.ts` interfaces + map thrown native errors via the `[code|exitCode]` message into `AutoAgentError`; async `run`/`evolve` already return Promises). Test (`node --test`): `doctor(root)` typed as `DoctorReport`, `apply(approve:false)` throws `AutoAgentError` with `code` starting `policy`. TDD. Commit.

### Task S3-T3: `AutoAgent` client class + types test

**Files:** `sdk/node/src/client.ts` (`new AutoAgent(root)`), `sdk/node/src/index.ts` re-export. Tests: client methods + a `tsc --noEmit` type-check gate proving the public API is fully typed. Commit.

**S3 exit criteria:** `import { doctor, AutoAgent } from '@autoagent/sdk'` returns typed objects; `tsc --noEmit` clean; node tests green.

---

## Milestone S4 — Deno SDK (`sdk/deno`)

### Task S4-T1: Typed Deno module over the native FFI

**Files:** Create `sdk/deno/deno.json` (exports `./mod.ts`), `sdk/deno/errors.ts`, `sdk/deno/mod.ts`. The generated `sdk/deno/_models.ts` (S1-T4) is the model source. `mod.ts` imports the native FFI wrapper (`crates/autoagent-bingen/deno/mod.ts` or the published JSR `@autoagent/native`), parses results into the typed models, maps the `1`+JSON tagged errors → `AutoAgentError`, and exposes the functional API. TDD (`deno test --allow-ffi`). Commit.

### Task S4-T2: `AutoAgent` client class + deno check

**Files:** `sdk/deno/client.ts` (`new AutoAgent(root)`), re-export from `mod.ts`. Tests + `deno check mod.ts` (type gate). Commit.

**S4 exit criteria:** `import { doctor, AutoAgent } from "jsr:@autoagent/sdk"` (or local `mod.ts`) returns typed models; `deno check` clean; `deno test --allow-ffi` green.

---

## Milestone S5 — Packaging, CI, publish, docs

### Task S5-T1: Per-SDK READMEs + examples

**Files:** `sdk/python/README.md`, `sdk/node/README.md`, `sdk/deno/README.md` — install + a runnable example per language (doctor + an approved run). Commit.

### Task S5-T2: Extend CI — build + test each SDK on the matrix

**Files:** Modify `.github/workflows/bingen.yml` (or new `sdk.yml`): jobs that build the native binding, then build + test each SDK (`maturin develop` + pytest; `npm ci` + `npm test` + `tsc`; `deno test` + `deno check`), and a **drift gate** (`bingen check` covers the generated `sdk/**/_models.*`). Validate YAML. Commit.

### Task S5-T3: Extend release — publish the SDK packages

**Files:** Modify `.github/workflows/bingen-release.yml`: publish `sdk/python` (PyPI, the mixed maturin wheel bundling `_native` + the pure-Python wrapper), `sdk/node` (`@autoagent/sdk` npm, depending on `@autoagent/native`), `sdk/deno` (JSR `@autoagent/sdk`). Dry-run validations. Commit.

**S5 exit criteria:** all three SDKs build + test in CI across the matrix; drift gate covers generated models; release workflow publishes the SDK packages.

---

## Final verification (before declaring complete)
1. `cargo test --workspace` green; `bingen check` clean (models drift-guarded).
2. Python: `maturin develop` in `sdk/python`; `pytest` green; `import autoagent; autoagent.doctor(root)` returns a `DoctorReport` (not a str).
3. Node: `npm test` + `tsc --noEmit` green; typed objects + `AutoAgentError`.
4. Deno: `deno test --allow-ffi` + `deno check` green.
5. Safety preserved: fail-closed approval raises/throws `AutoAgentError` (code `policy.*`) through every SDK; SDKs add no capability beyond the bindings.
6. A real round-trip per language parses a live native result into every typed model field (drift test).

## Deferred (note, NOT in scope)
- Streaming run/evolve progress events (the bindings return final outcomes only).
- node-bindgen / RustPython / raw-FFI as SDK backends (the SDKs target the primaries: pyo3, napi, deno_bindgen/FFI).
- Improving the native `.pyi` boolean typing (the SDK supersedes it with real models).
