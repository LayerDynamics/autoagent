# SPEC-2: AutoAgent Binding Generator (`autoagent-bingen`)

> A single-source-of-truth binding generator that decorates `autoagent-core`'s public surface once and compiles it into loadable Python, Node.js, and Deno modules across six backends — without ever bypassing AutoAgent's safety engine.

**Date:** 2026-06-09
**Author:** Ryan O'Boyle + Claude
**Status:** Draft
**Version:** 1.0
**Repository:** <https://github.com/LayerDynamics/autoagent>
**Crate:** `crates/autoagent-bingen`
**Depends on:** [SPEC-1: AutoAgent](./SPEC-1-autoagent.md) (the engine being bound)

---

## 0. Implementation Status (2026-06-10)

The crate is implemented (B1–B5). What differs from the original draft below:

- **All six backends build, load, and run** with fail-closed safety, each verified
  end-to-end (`crates/autoagent-bingen/scripts/smoke-all.sh`, exit 0):
  napi-rs, node-bindgen, pyo3, RustPython, deno_bindgen, raw FFI.
- **R-1 / Q-1 RESOLVED — RustPython works for real** as a CPython-free native
  `#[rustpython_vm::pymodule]` installed via
  `Interpreter::builder(..).add_native_module(module_def(&ctx))`. It is *not*
  "experimental / pyo3-only"; the §App-B "experimental" tag and the §3.1 diagram
  `[+ rustpython, exp.]` annotation are superseded by this section.
- **node-bindgen REALITY:** nj-core 6.1's old `#[ctor]` `napi_module_register`
  is not honored by Node 18+ and is dead-stripped. The generated backend exports
  `napi_register_module_v1` delegating to nj-core's `init_modules`, and the
  cdylib is built with `RUSTFLAGS="-C link-dead-code"`. R-2's deno_bindgen note
  also realized: its CLI can't locate `bindings.json` in a workspace+build.rs
  crate, so `deno/gen.ts` calls deno_bindgen's `codegen()` directly.
- **FR-5 (async `run`/`evolve` as Promise/awaitable) is PARTIAL.** The sync
  surface ships and is wired (`run_sync`/`evolve_sync`, plus `apply`/`revert`),
  fully fail-closed. The async Promise/awaitable variants are an open item
  tracked as B3-T6 — decoupled deliberately so async ergonomics never gate the
  safety guarantees. **This is the one remaining MUST not yet met.**
- **Safety-parity E2E covers `apply` (binding == real CLI, byte-identical patch).**
  `evolve --apply` (self-authoring) parity is not yet covered by an E2E and is a
  follow-up.

---

## 1. Background

### 1.1 Problem Statement

AutoAgent's entire value — controlled, reversible, policy-gated codebase mutation — lives in the Rust crate `autoagent-core` and is reachable today only through the `autoagent` CLI binary. A large population of would-be consumers (Node.js build tooling, Deno scripts, Python automation, CI scripts, agent frameworks, notebooks) cannot call that engine without shelling out to the CLI, parsing its stdout, and losing typed results, structured errors, and in-process control over approval gates.

The unsolved problem is **safe, typed, multi-language reach**: how to expose the same engine to Python, Node.js, and Deno as first-class loadable modules — with the *identical* safety guarantees the CLI enforces — while maintaining only **one** authoritative definition of the bound surface, so the six binding backends never drift from each other or from core.

The defining stance of `autoagent-bingen` mirrors SPEC-1's: **bindings are a transport layer, never an authority layer.** A Python, Node, or Deno caller gets the same PolicyEngine, the same snapshot-before-write, the same audit log, and the same `revert` as a CLI user. The bindings add language ergonomics and subtract nothing from safety. This identity is load-bearing and shapes every requirement below.

### 1.2 Current State

- **CLI-only access.** `autoagent-core` is consumed exclusively by `autoagent-cli` (the `autoagent` binary). Any non-Rust caller must spawn a subprocess, pass a JSON plan on disk or argv, and scrape exit codes / text — losing the typed `RunOutcome`, `DoctorReport`, `ValidationReport`, and the structured `AutoAgentError` code taxonomy.
- **No language SDKs.** There is no npm package, no Deno/JSR module, and no PyPI wheel for AutoAgent. Agent frameworks and automation written in JS/TS, Deno, or Python cannot embed the engine.
- **The crate is empty scaffolding.** `crates/autoagent-bingen` exists with the directory tree and zero-byte files (`bind.rs`, `build.rs`, `Cargo.toml`, `src/main.rs`, `src/node/{mod,napi,node_bindgen}.rs`, `src/python/{mod,pyrs,python_bingen}.rs`, `src/deno/{mod,deno_bindgen,ffi}.rs`) but is **not** a workspace member and contains no code.
- **Hand-written multi-backend bindings rot.** The naive alternative — writing napi-rs, node-bindgen, pyo3, RustPython, deno_bindgen, and raw-FFI wrappers by hand — produces six parallel surfaces that drift the moment core changes. There is no mechanism today to keep six backends + stub languages in lockstep with one Rust API.

Neither path treats the cross-language boundary as a *generated artifact of a single declarative surface*. `autoagent-bingen` makes that the architectural center: `bind.rs` is the one place the surface is declared and marshaled; everything else (six backends across three languages, the stub dialects, schema, scaffolds) is generated from it.

### 1.3 Target Users

Primary users: **developers integrating AutoAgent into a non-Rust toolchain** —
- **Node.js / TypeScript** authors of build plugins, CLIs, and agent orchestrators who want `await aa.run(plan)` with typed results and `Promise` semantics.
- **Deno / TypeScript** authors who want a permissioned, URL/JSR-imported module (`import { run } from "jsr:@autoagent/native"`) loaded via Deno FFI, with TS types out of the box.
- **Python** authors of automation, notebooks, and agent frameworks who want `import autoagent` with both `async` and `_sync` entrypoints and `.pyi` autocomplete.

Secondary users:
- **AutoAgent maintainers**, who edit one registry (`bind.rs`) and regenerate all six backends + stubs + schema in a single step, with a smoke harness proving the wiring.
- **Tooling authors** who consume the exported JSON Schema to codegen their own clients in languages this crate does not bind.

### 1.4 Motivation

- **Reach without rewrite:** the engine already exists and is tested (SPEC-1, 1.0.0). Bindings multiply its addressable surface across the two largest scripting ecosystems with zero reimplementation of safety logic.
- **One surface, many targets:** generating backends from a single registry eliminates the dominant failure mode of multi-language FFI projects — backend drift — and makes "add a command" a one-edit operation.
- **Safety parity as a differentiator:** competing "call Rust from Python/JS" wrappers expose raw capability. AutoAgent's bindings are differentiated precisely because they *cannot* weaken the policy/snapshot/audit invariants — the same reason the CLI is trustworthy.
- **Distribution leverage:** prebuilt npm + PyPI artifacts turn "clone and `cargo build`" into `npm install` / `pip install`, which is the difference between a demo and an adopted SDK.

### 1.5 Assumptions

- `autoagent-core`'s public API is the binding contract. The bound surface is a curated subset of `pub` items in `autoagent-core` (the workflow entrypoints), not its entire internals.
- Marshaling is **JSON-at-the-boundary** by default; richer native object mapping is an optimization, not a correctness requirement. Some core boundary types already derive `serde` (confirmed: `ProjectAnalysis`, `Plan`, `ValidationReport`); others currently do **not** and require an additive `#[derive(Serialize, Deserialize)]` with no behavior change (confirmed missing: `DoctorReport`/`Check`, `RunOutcome` — which nests `RunState` — and `AutoAgentError`, which derives only `thiserror::Error`). This additive-derive work is tracked in R-8 and is a precondition for B1/B2.
- The host toolchains are present for source builds: a Rust 1.78+ toolchain, Node.js ≥ 18 (N-API 8), Deno ≥ 1.40 (stable FFI, `--allow-ffi`), and CPython ≥ 3.9 (abi3). Prebuilt artifacts remove this requirement for consumers.
- `bind.rs` is the **single source of truth**; the six backend `.rs` files, the `.d.ts`/`.pyi` stubs, the JSON Schema, and the package scaffolds are **generated** by the `bingen` binary (`main.rs`) and are not hand-edited.
- The safety boundary is core's, not the binding's: bindings construct the same `PolicyEngine` from the same `Autoagent.toml` and call the same workflow functions. Approval is surfaced to the host as a callback whose **default-on-absence is refuse**, matching the CLI's non-`--yes` behavior.
- RustPython is treated as an **alternative, CPython-independent** Python backend (embedding the bound surface into a RustPython interpreter); pyo3 is the primary, CPython-native path. Exact RustPython exposure mechanics are an open question (§8), not a blocker for the primary backends.
- Deno consumes a **C-ABI `cdylib`** via its FFI (`Deno.dlopen`). Because FFI passes only C scalars/pointers, the JSON-at-the-boundary contract crosses as a `(ptr, len)` C string with a paired free function; the generated TypeScript wrapper hides this and presents the same typed surface as the Node `.d.ts`. `deno_bindgen` (primary Deno backend) generates that wrapper; the raw `ffi.rs` path is the dependency-free alternative.

---

## 2. Requirements

### 2.1 Functional Requirements

| ID | Priority | Requirement |
|----|----------|-------------|
| FR-1 | MUST | `bind.rs` MUST be the single declarative source of truth for the bound surface: every exported function/type is declared once, with its name, argument shape, return shape, sync/async kind, and privilege class. |
| FR-2 | MUST | The `bingen` binary (`src/main.rs`) MUST read `bind.rs` and generate the six backend modules (`node/napi.rs`, `node/node_bindgen.rs`, `python/pyrs.rs`, `python/python_bingen.rs`, `deno/deno_bindgen.rs`, `deno/ffi.rs`), the type stubs, the JSON Schema, and the package scaffolds. Generated files MUST carry a "do not edit — generated by autoagent-bingen" header. |
| FR-3 | MUST | The crate MUST produce **six** loadable backends across **three** languages, all shippable: Node via **napi-rs** (`.node`) and via **node-bindgen** (`.node`); Python via **pyo3** (abi3 wheel) and via **RustPython** embedding (`python_bingen`); Deno via **deno_bindgen** (C-ABI `cdylib` + TS wrapper) and via raw **FFI** (`Deno.dlopen` against the same `cdylib`). |
| FR-4 | MUST | The bound surface MUST achieve **full CLI parity**, exposing: `init`, `doctor`, `analyze`, `plan` (generate/validate/read/write), `apply`, `run`, `evolve`, `patch`, `revert`, `memory`, `config`. |
| FR-5 | MUST | Async core entrypoints (`run_workflow`, `evolve_generated`, `planner::generate_plan`) MUST be exposed as JS `Promise`s and as Python `async` functions, each with a `*_sync` blocking variant for Python and a `*Sync` variant for Node where a blocking call is ergonomic. |
| FR-6 | MUST | Every mutating call (`apply`, `run`, `evolve`, `revert`, `patch`) MUST route through the same `PolicyEngine`, snapshot-before-write, and event log as the CLI. The binding layer MUST NOT expose any path that mutates the workspace without passing the policy engine (SPEC-1 FR-24 parity). |
| FR-7 | MUST | Approval gates MUST be surfaced to the host runtime as a callback (JS/Deno function / Python callable; Deno FFI via `Deno.UnsafeCallback`). If no callback is supplied, the binding MUST behave as non-interactive and **refuse** privileged operations unless an explicit `approve: true` / `auto_approve=True` option is set (matching CLI `--yes` semantics). |
| FR-8 | MUST | `AutoAgentError` MUST be mapped to idiomatic host errors: a JS `Error` subclass and a Python `Exception` subclass, each carrying the stable `code` string (e.g. `policy.path_escape`) and the numeric `exit_code` from core's taxonomy. |
| FR-9 | MUST | The pipeline MUST generate **TypeScript `.d.ts`** (shared by Node and Deno), a **Deno TS FFI wrapper** (`mod.ts` presenting the typed surface over `Deno.dlopen`), and **PEP 561 `.pyi`** stubs — covering every exported symbol with accurate argument and return types. |
| FR-10 | MUST | The pipeline MUST generate **package scaffolds**: `package.json` (napi triples + binary entry + loader), `pyproject.toml` (maturin/abi3), `deno.json` + the Deno `mod.ts` entry, and loader shims (`index.js`, `__init__.py`) that select the correct prebuilt binary per platform. |
| FR-11 | MUST | The pipeline MUST emit a **machine-readable JSON Schema** of the bound surface (each command's name, args, return shape, privilege class) so external tools can codegen clients. |
| FR-12 | SHOULD | `src/main.rs` SHOULD provide a `smoke` subcommand that loads each compiled backend and exercises a non-mutating call (`doctor`, `analyze`) to validate binding wiring in CI. |
| FR-13 | MUST | Prebuilt artifacts MUST be published: an npm package (`@autoagent/native`) with per-platform `.node` prebuilds, PyPI abi3 wheels (`autoagent`), and a Deno/JSR module (`@autoagent/native`) that downloads/loads the matching `cdylib` — each with a source-build fallback. |
| FR-14 | MUST | CI MUST build and test the artifact matrix across **linux, macOS, windows × x64, arm64** for all three ecosystems (Node, Python, Deno). |
| FR-15 | SHOULD | The generator SHOULD enforce **surface/version parity**: a check that fails the build if `bind.rs` references a core symbol that no longer exists, or if the generated stubs are out of date relative to `bind.rs` (drift guard). |
| FR-16 | SHOULD | The crate SHOULD expose a `version()` / `schemaVersion()` accessor returning core's `schema_version`, so host code can assert compatibility at load time. |
| FR-17 | COULD | The generator COULD emit a third stub flavor (e.g. a `.proto` or OpenAPI-style descriptor) from the same registry for non-JS/Python consumers. |
| FR-18 | COULD | The bindings COULD stream long-running `run`/`evolve` progress events to the host via an async iterator / callback channel rather than returning only the final outcome. |
| FR-19 | WONT | The bindings WILL NOT add any capability absent from `autoagent-core` (no new file-mutation, command-execution, or network path that the CLI does not also have). |
| FR-20 | WONT | The bindings WILL NOT allow a privileged operation to proceed without either an approval callback grant or an explicit pre-authorization flag — there is no "force" escape hatch. |
| FR-21 | WONT | This spec WILL NOT cover hand-editing the generated backend files; any change to the surface flows through `bind.rs` and regeneration. |

### 2.2 Non-Functional Requirements

> NFR targets marked *(provisional)* are proposed defaults pending owner confirmation (see §8 Open Questions).

#### Performance

| Metric | Target | Measurement |
|--------|--------|-------------|
| Binding call overhead (non-mutating, e.g. `doctor`) | p95 added latency < **2 ms** over a direct in-process Rust call *(provisional)* | Criterion micro-bench: Rust direct vs napi/pyo3 call, same workspace |
| Marshaling overhead for a typical `analyze` result (~100 files) | < **10 ms** to serialize+deserialize at the boundary *(provisional)* | Bench on a fixed fixture workspace |
| Cold module load (native addon import) | < **150 ms** on a warm OS cache *(provisional)* | `require('@autoagent/native')` / `import autoagent` timed in CI |
| Async call does not block the host event loop | 0 ms main-thread block during `run`/`evolve` | napi-rs `AsyncTask` / pyo3 `future_into_py`; verified by a concurrency test |

#### Reliability

| Metric | Target |
|--------|--------|
| Safety parity | 100% of mutating ops route through PolicyEngine + snapshot + audit log (enforced by test, no bypass path exists) |
| Surface parity | Generated backends/stubs are byte-identical to a fresh regeneration from `bind.rs` (drift guard, FR-15) |
| Error fidelity | 100% of `AutoAgentError` variants map to a host error carrying the correct `code` and `exit_code` |
| Reversibility parity | A `run`/`evolve` initiated via bindings is revertible via the bindings' `revert` with the same guarantees as the CLI |

#### Security & Compliance

- **No privilege escalation:** the binding layer adds no syscall, file, or command capability beyond `autoagent-core`. The PolicyEngine is the sole authority; bindings cannot construct an engine with weakened policy except by reading the same `Autoagent.toml`.
- **Approval integrity:** absent an approval callback or explicit pre-authorization, privileged ops refuse (fail-closed). There is no environment variable or option that disables the policy engine.
- **Audit continuity:** every binding-initiated run writes the same `.agent/runs/<id>/` audit trail and event log as the CLI, attributable and reviewable.
- **Supply-chain:** prebuilt artifacts are built only in CI from a tagged commit; checksums published. No `build.rs` network access at consumer install time for prebuilt paths.
- **Data sensitivity:** operates on local source trees only (inherits SPEC-1 §3.7). No new data egress.

#### Scalability

- The generator MUST handle a growing surface (target: **≥ 50 exported symbols** without manual per-backend edits). Adding a command is O(1) edits to `bind.rs`.
- The artifact matrix MUST scale to additional targets (e.g. musl, freebsd) by adding CI matrix entries, not code.

### 2.3 Constraints

- **Language/runtime:** Rust (workspace edition 2021, rust-version 1.78), compiled to `cdylib`/`staticlib` as each backend requires. Node N-API 8+ (Node ≥ 18). CPython abi3 ≥ 3.9.
- **Workspace integration:** `autoagent-bingen` MUST be added to the root `Cargo.toml` `[workspace] members` and use `version.workspace`/`edition.workspace`/`license.workspace`.
- **Dependency direction:** `autoagent-bingen` depends on `autoagent-core` (and transitively `autoagent-plugin-sdk`); nothing in core/cli depends back on bingen.
- **Backend crates:** napi-rs (`napi`, `napi-derive`, `napi-build`), node-bindgen (`node-bindgen`, `nj-build`), pyo3 (`pyo3` w/ `abi3-py39`, `extension-module`), RustPython (`rustpython-vm`), deno_bindgen (`deno_bindgen` + CLI), and a raw-FFI path (`std`-only `extern "C"`). All gated behind Cargo features so a single build can target one backend.
- **No core changes for binding's sake** beyond additive `#[derive(Serialize, Deserialize)]` / `pub` exposure where a boundary type currently lacks it; such changes are tracked and must not alter core behavior.
- **Licensing:** MIT (workspace), compatible with all six backend crates'/toolchains' licenses (verify in audit, §8).

### 2.4 Explicit Non-Goals

- Not a general-purpose Rust↔(JS/Python) FFI framework; it binds *AutoAgent's* surface only.
- Not a reimplementation of any engine logic in JS/Python; all logic stays in `autoagent-core`.
- Not an interactive TUI/CLI for non-Rust users; the CLI (`autoagent-cli`) remains the human-facing terminal tool.
- Not a network/RPC server exposing AutoAgent over a socket (that would be a separate spec); these are in-process native modules.
- Not a hand-maintained set of bindings; generation from `bind.rs` is mandatory (FR-21).

---

## 3. Architecture

### 3.1 System Overview

`autoagent-bingen` is a **generator + multi-backend binding crate**. One declarative registry (`bind.rs`) describes the surface and holds the centralized marshaling/error/async-bridge logic over `autoagent-core`. The `bingen` binary (`main.rs`) consumes that registry to emit six backend adapters (across Node, Python, Deno), the stub dialects, a JSON Schema, and package scaffolds. Each backend adapter compiles (behind a Cargo feature) into a loadable native module.

```
                         autoagent-core (SPEC-1 engine; sole authority)
                                   ▲  init/doctor/analyze/plan/apply/
                                   │  run/evolve/patch/revert/memory/config
                                   │
   ┌───────────────────────────────┴────────────────────────────────┐
   │  autoagent-bingen                                               │
   │                                                                 │
   │   bind.rs  ── SINGLE SOURCE OF TRUTH ───────────────────────┐   │
   │     • surface registry (symbols, arg/ret shapes, kind,      │   │
   │       privilege class)                                      │   │
   │     • centralized marshaling (serde JSON at boundary)       │   │
   │     • AaError → host-error mapping (code + exit_code)       │   │
   │     • async bridge (Future ↔ Promise / asyncio)             │   │
   │     • approval-callback plumbing (fail-closed)              │   │
   │                          │ read by                          │   │
   │                          ▼                                  │   │
   │   main.rs (bingen) ── GENERATES ──────────────────────────┐ │   │
   │     generate | smoke | check  subcommands                 │ │   │
   │     │        │         │        │        │        │        │ │   │
   │     ▼        ▼         ▼        ▼        ▼        ▼        │ │   │
   │  node/    node/     python/  python/   deno/     deno/    │ │   │
   │  napi.rs  node_     pyrs.rs  python_   deno_     ffi.rs   │ │   │
   │  (#[napi])bindgen   (#[pyo3])bingen    bindgen   (C-ABI   │ │   │
   │           (.rs)              (RustPy)  (TS wrap) dlopen)  │ │   │
   │     │        │         │        │        │        │        │ │   │
   │  + index.d.ts (Node+Deno)  + __init__.pyi  + mod.ts      │ │   │
   │  + package.json + pyproject.toml + deno.json + surface.json│ │   │
   │  + loader shims (index.js / __init__.py / mod.ts)         │ │   │
   │                          │ compiled by                     │ │   │
   │                          ▼                                  │   │
   │   build.rs ── selects backend feature, runs codegen,       │   │
   │               configures napi/nj/pyo3/cdylib build          │   │
   └─────────────────────────┬───────────────────────────────────┘   │
                             ▼
   Artifacts:  @autoagent/native (.node)  |  autoagent (abi3 wheel)       |  jsr:@autoagent/native
               napi + node-bindgen        |  pyo3 [+ rustpython, exp.]    |  deno_bindgen + FFI (cdylib)
```

### 3.2 Component Design

#### Component: `bind.rs` — Surface Registry & Centralized Logic
- **Responsibility:** Be the one authoritative declaration of the bound surface and house all backend-neutral marshaling, error-mapping, async-bridging, and approval plumbing.
- **Technology:** Rust; a declarative registry (const tables / a `surface!{}` macro) plus neutral wrapper functions `fn <op>(args_json) -> Result<ret_json, AaError>` that call `autoagent-core`.
- **Interfaces:** Exposes (a) machine-readable surface metadata for `main.rs` codegen, and (b) callable neutral wrappers the generated backends delegate to.
- **Dependencies:** `autoagent-core`, `serde`, `serde_json`.

#### Component: `main.rs` — the `bingen` Binary (Generator)
- **Responsibility:** Read the surface registry and generate all backend adapters, stubs, schema, and scaffolds; run smoke and drift-check.
- **Technology:** Rust bin (`[[bin]] name = "bingen"`). Subcommands: `generate`, `check` (drift guard, FR-15), `smoke` (FR-12).
- **Interfaces:** CLI; writes generated files into `src/node/`, `src/python/`, and `dist/` (stubs/scaffolds/schema).
- **Dependencies:** `bind.rs` (same crate), a code-emitter (string templating or `quote`/`prettyplease` for `.rs`).

#### Component: `build.rs` — Build Orchestrator
- **Responsibility:** Select the active backend via Cargo feature, run codegen if stale, configure the chosen backend's build (napi-build / nj-build / pyo3 abi3), and set the right crate type.
- **Technology:** Rust build script.
- **Interfaces:** Cargo build hooks; emits `cargo:` directives.
- **Dependencies:** `napi-build`, `nj-build`, `pyo3-build-config` (feature-gated).

#### Component: `node/napi.rs` — napi-rs Adapter (generated)
- **Responsibility:** Re-export every surface symbol with `#[napi]`, bridging async to `AsyncTask`/`Promise` and `AaError` to a JS `Error`.
- **Technology:** napi-rs (`napi`, `napi-derive`). Crate type `cdylib` → `.node`.
- **Interfaces:** Node N-API module.
- **Dependencies:** `bind.rs`, `napi`.

#### Component: `node/node_bindgen.rs` — node-bindgen Adapter (generated)
- **Responsibility:** Same surface via infinyon `node-bindgen` (`#[node_bindgen]`) as the alternative Node backend.
- **Technology:** `node-bindgen`, `nj-build`. Crate type `cdylib` → `.node`.
- **Interfaces:** Node N-API module (alternative ABI/codegen path).
- **Dependencies:** `bind.rs`, `node-bindgen`.

#### Component: `python/pyrs.rs` — pyo3 Adapter (generated, primary Python)
- **Responsibility:** Re-export the surface as a `#[pymodule]` of `#[pyfunction]`s; async via `pyo3-asyncio`/`future_into_py` + `_sync` variants; `AaError` → Python exception subclass.
- **Technology:** pyo3 (abi3-py39, extension-module). Crate type `cdylib` → `.so`/`.pyd`.
- **Interfaces:** CPython extension module.
- **Dependencies:** `bind.rs`, `pyo3`.

#### Component: `python/python_bingen.rs` — RustPython Adapter (generated, alternative Python)
- **Responsibility:** Expose the surface to a RustPython interpreter as a CPython-independent path (the in-house "bingen" Python backend).
- **Technology:** `rustpython-vm` (feature-gated).
- **Interfaces:** RustPython native module / embedding API.
- **Dependencies:** `bind.rs`, `rustpython-vm`.
- **Note:** Exact exposure mechanism is an open question (§8); this component is the alternative, not the primary Python path.

#### Component: `deno/deno_bindgen.rs` — deno_bindgen Adapter (generated, primary Deno)
- **Responsibility:** Annotate the surface with `#[deno_bindgen]` so the toolchain emits a C-ABI `cdylib` plus a typed TypeScript `mod.ts` wrapper that calls it via `Deno.dlopen`.
- **Technology:** `deno_bindgen` crate + CLI. Crate type `cdylib`.
- **Interfaces:** Deno FFI symbols (C ABI) + generated TS wrapper.
- **Dependencies:** `bind.rs`, `deno_bindgen`.

#### Component: `deno/ffi.rs` — Raw FFI Adapter (generated, alternative Deno)
- **Responsibility:** Export each surface symbol as a stable `#[no_mangle] extern "C"` function over the `(ptr, len)` JSON-string ABI (plus a `free` export), for consumers who want a dependency-free `Deno.dlopen` binding without the `deno_bindgen` toolchain.
- **Technology:** Rust `extern "C"` over the same `bind.rs` neutral wrappers. Crate type `cdylib`.
- **Interfaces:** C ABI consumed by a hand-shaped or generated `mod.ts`.
- **Dependencies:** `bind.rs`.

#### Component: `node/mod.rs`, `python/mod.rs`, `deno/mod.rs` — Backend Module Gates
- **Responsibility:** Feature-gate and re-export the active backend(s); hold shared per-language helpers (loader hints, the C-ABI string/free helpers for Deno, type-conversion glue not in `bind.rs`).
- **Technology:** Rust modules with `#[cfg(feature = ...)]`.
- **Dependencies:** sibling backend files.

#### Component: Generated Artifacts (stubs / scaffolds / schema)
- **Responsibility:** Provide consumer-facing typing and packaging. `index.d.ts`, `__init__.pyi`, `package.json`, `pyproject.toml`, loader shims, `surface.schema.json`.
- **Technology:** Emitted text from the registry.
- **Dependencies:** surface metadata from `bind.rs`.

### 3.3 Data Model

The crate introduces **no persistent storage**; it marshals core's existing entities across the boundary. Key boundary entities (all `serde`-serializable in core):

| Entity (core type) | Crosses as | Notes |
|--------------------|------------|-------|
| `DoctorReport { checks: [Check], all_ok }` | JSON object | `doctor()` return |
| `ProjectAnalysis` | JSON object | `analyze()` return |
| `Plan` (+ `plan_json_schema()`) | JSON object | `plan` generate/validate/read/write |
| `RunOutcome { run_id, final_state: RunState, report: ValidationReport }` | JSON object | `run`/`evolve` return |
| `ValidationReport { passed, ... }` | JSON object | nested in outcomes |
| `AutoAgentError` / `PolicyError` | host error (`code: string`, `exit_code: number`, `message`) | error mapping (FR-8) |
| Approval request | host callback arg `{ kind, target, command? }` → `bool` | approval gate (FR-7) |
| **Surface descriptor** (bingen-owned) | `surface.schema.json` | the only *new* schema: `{ symbol, args[], returns, kind: sync\|async, privilege: read\|mutate }` |

The **surface descriptor** is the one new data structure this crate owns. It is the serialized form of the `bind.rs` registry and the contract the generator and external codegen consume.

### 3.4 API & Interface Design

**Bound surface (full CLI parity, FR-4).** Illustrative signatures (TypeScript / Python); exact arg objects are generated from the registry:

```typescript
// @autoagent/native  (index.d.ts, generated)
export function init(root: string, opts?: { yes?: boolean }): boolean;
export function doctor(root: string): DoctorReport;
export function analyze(root: string): ProjectAnalysis;
export namespace plan {
  function validate(planJson: object, root: string): void;       // throws AaError
  function read(path: string): Plan;
  function write(root: string, slug: string, plan: Plan): { planPath: string; mdPath: string };
}
export function apply(root: string, planPath: string, opts?: ApproveOpts): RunOutcome;
export function run(root: string, planPath: string, opts?: ApproveOpts): Promise<RunOutcome>;
export function runSync(root: string, planPath: string, opts?: ApproveOpts): RunOutcome;
export function evolve(root: string, opts: EvolveOpts & ApproveOpts): Promise<RunOutcome>;
export function revert(root: string, runId: string): void;
export function version(): string;          // core schema_version
export class AutoAgentError extends Error { code: string; exitCode: number; }
export interface ApproveOpts { approve?: boolean; onApproval?: (req: ApprovalRequest) => boolean; }
```

```python
# autoagent/__init__.pyi  (generated)
def init(root: str, *, yes: bool = False) -> bool: ...
def doctor(root: str) -> DoctorReport: ...
def analyze(root: str) -> ProjectAnalysis: ...
async def run(root: str, plan_path: str, *, approve: bool = False,
              on_approval: Callable[[ApprovalRequest], bool] | None = None) -> RunOutcome: ...
def run_sync(root: str, plan_path: str, *, approve: bool = False) -> RunOutcome: ...
async def evolve(root: str, **opts) -> RunOutcome: ...
def revert(root: str, run_id: str) -> None: ...
def version() -> str: ...
class AutoAgentError(Exception):
    code: str        # e.g. "policy.path_escape"
    exit_code: int
```

```typescript
// jsr:@autoagent/native  (Deno mod.ts, generated over Deno.dlopen / deno_bindgen)
// Requires: deno run --allow-ffi --allow-read --allow-write
import { doctor, run, type RunOutcome, AutoAgentError } from "jsr:@autoagent/native";
const report = doctor(Deno.cwd());                 // sync FFI call
const outcome: RunOutcome = await run(Deno.cwd(), "plan.json", { approve: true });
// same typed surface as Node's index.d.ts; FFI (ptr,len) JSON marshaling is hidden by mod.ts
```

**Generator CLI:**

```text
bingen generate            # emit backends + stubs + schema + scaffolds from bind.rs
bingen check               # drift guard: fail if generated files != fresh regen (FR-15)
bingen smoke               # load each compiled backend, call doctor/analyze (FR-12)
```

**Surface schema (`surface.schema.json`, FR-11):**

```json
{
  "version": "1.0.0",
  "symbols": [
    { "name": "run", "kind": "async", "privilege": "mutate",
      "args": [{ "name": "root", "type": "string" },
               { "name": "plan_path", "type": "string" },
               { "name": "opts", "type": "ApproveOpts" }],
      "returns": "RunOutcome" }
  ]
}
```

**Surface composition note.** Several "commands" are facades over more than one core function, and `bind.rs` is where that mapping is declared:
- `run`/`evolve` → `run_workflow`/`run_with_plan`, `evolve_generated`/`evolve_with_plan` (confirmed in `runtime/`).
- `analyze`/`doctor`/`init`/`revert` → the single confirmed entrypoints `analysis::analyze`, `runtime::doctor::doctor`, `runtime::init::init_workspace`, `runtime::revert::revert`.
- `plan` → `planner::generate_plan`, `plan_validator::validate_plan`, `plan_reader::read_plan`, `plan_writer::write_plan` (confirmed).
- `memory` → `MemoryStore` methods (`load_project`/`save_project`/`load_decisions`/`append_decision`/…) and `project_memory::{rebuild_project_memory, recent_decision_summaries}` (confirmed).
- `config` → `AutoAgentConfig::{load, from_toml_str}` + `default_toml` (confirmed).
- `patch` → core's patch artifacts are produced by `editing::patch_writer`; the exact read/list/apply entrypoints the `patch` command should bind are **to be confirmed against the CLI's `patch` handler** (tracked as Q-10).

The bound surface is therefore a curated, declared façade — not a blind 1:1 re-export of every core `pub fn`.

### 3.5 Data Flow

**Flow A — Non-mutating call (`doctor` from Node):**
```
JS doctor(root) → napi.rs #[napi] doctor → bind::doctor(root_json)
  → core::runtime::doctor::doctor(&Utf8Path) → DoctorReport
  → serde_json serialize → napi return → JS object
```

**Flow B — Mutating async call with approval (`run` from Python):**
```
py: await run(root, plan_path, on_approval=cb)
  → pyrs.rs #[pyfunction] (future_into_py)
  → bind::run(args, approval_bridge=cb)
  → core PolicyEngine constructed from Autoagent.toml
       │  on each privileged step → approval_bridge → cb(req) → bool
       │     (no cb supplied & approve=False  →  refuse  →  AaError::Policy)
  → snapshot-before-write → apply → validate → (bounded repair) → event_log
  → RunOutcome → serde → Python object  (or AutoAgentError raised on failure)
```

**Flow C — Deno FFI call (`doctor` from Deno):**
```
ts: doctor(root)  [mod.ts]
  → Deno.dlopen(libautoagent_bingen, symbols).doctor(rootPtr, rootLen)
  → ffi.rs / deno_bindgen.rs  extern "C"  → bind::doctor(root_json)
  → core doctor → DoctorReport → serde → CString (ptr,len) returned
  → mod.ts reads (ptr,len) → JSON.parse → typed object → calls free(ptr)
```

**Flow D — Generation (build/dev time):**
```
bind.rs registry → main.rs generate
  → emit src/node/{napi,node_bindgen}.rs, src/python/{pyrs,python_bingen}.rs,
         src/deno/{deno_bindgen,ffi}.rs
  → emit dist/index.d.ts (Node+Deno), autoagent/__init__.pyi, deno/mod.ts
  → emit package.json, pyproject.toml, deno.json, loader shims
  → emit surface.schema.json
build.rs (feature=node-napi) → napi-build → compile cdylib → .node
build.rs (feature=deno)      → compile cdylib → lib*.{so,dylib,dll} (Deno.dlopen)
```

### 3.6 Integration Points

- **`autoagent-core`** — the bound engine (sole inbound dependency, sole authority).
- **`autoagent-plugin-sdk`** — transitive via core; the bound surface does not expose plugin internals beyond what core exposes.
- **napi-rs / node-bindgen / pyo3 / rustpython / deno_bindgen / raw FFI** — the six backend mechanisms.
- **npm registry** (`@autoagent/native`), **PyPI** (`autoagent`), and **JSR/deno.land** (`@autoagent/native`) — artifact distribution (FR-13).
- **CI (GitHub Actions)** — artifact matrix build/test/publish (FR-14), reusing SPEC-1's CI/release workflow conventions.
- **`Autoagent.toml`** — read by bindings to construct the PolicyEngine (same config as CLI).

### 3.7 Security Architecture

- **Single authority:** all privileged behavior is delegated to `autoagent-core`'s PolicyEngine. The binding constructs the engine only from `Autoagent.toml`; it exposes no API to mutate policy in memory.
- **Fail-closed approval (FR-7, FR-20):** privileged ops require either an approval-callback grant or an explicit `approve`/`auto_approve` flag. Absent both → refuse with `AaError::Policy(...)`. No "force" path exists.
- **No new capability (FR-19):** the surface is a strict subset of core's public workflow functions; there is no `exec`/`spawn`/raw-`fs` binding that bypasses core.
- **Audit parity:** binding-initiated runs produce the identical `.agent/runs/<id>/` trail and event log; attribution and `revert` work the same.
- **Build supply chain:** prebuilt artifacts are produced only by CI from tagged commits with published checksums; consumer installs of prebuilt artifacts perform no network `build.rs`.
- **Error taxonomy preservation:** `code` + `exit_code` cross intact so host-side policy/observability can branch on the same denial categories (`policy.path_escape`, `policy.blocked_command`, …) the CLI uses.

### 3.8 Resilience Design

- **Async isolation:** long-running `run`/`evolve` execute off the host's main thread (napi `AsyncTask` / pyo3 `future_into_py`), so a host event loop is never blocked (NFR).
- **Error containment:** Rust panics at the boundary are caught and converted to a host error rather than aborting the host process (napi-rs catch / pyo3 `catch_unwind` wrapper); core uses `Result`, so panics should be exceptional.
- **No partial-state leak:** because mutation goes through core's snapshot/apply, a failed or refused binding call leaves the workspace in the same recoverable state the CLI guarantees.
- **Backpressure / cancellation (COULD, FR-18):** progress streaming and host-initiated cancellation are future extensions; v1 returns the final outcome.
- **Drift guard (FR-15):** `bingen check` in CI prevents shipping backends/stubs that disagree with `bind.rs`.

### 3.9 Observability

- **Event-log continuity:** binding-initiated runs write core's existing event log; no separate logging layer is introduced.
- **Generation provenance:** every generated file carries a header with the generator version and source-registry hash for traceability.
- **Smoke signal (FR-12):** `bingen smoke` emits per-backend load+call results, consumed by CI as a wiring health check.
- **Build matrix visibility:** CI reports per-target build/test status across the 6-cell platform matrix.
- **Bench tracking:** Criterion micro-benches for call/marshaling overhead run in CI to catch NFR regressions.

### 3.10 Infrastructure & Deployment

- **Workspace:** add `crates/autoagent-bingen` to root `[workspace] members`; inherit workspace version/edition/license.
- **Crate types per backend (feature-gated):** `cdylib` for napi/node-bindgen/pyo3 and for Deno (`deno_bindgen` + raw FFI load the `cdylib` via `Deno.dlopen`); `bin` for the generator; RustPython per its embedding model.
- **CI/CD:** GitHub Actions matrix (linux/macos/windows × x64/arm64) building `.node` prebuilds (napi-rs `prepublish`/artifacts), abi3 wheels (maturin `build`/cibuildwheel), and the Deno `cdylib` + `mod.ts` bundle. Publish: `npm publish @autoagent/native`, `twine upload` to PyPI, and `deno publish` to JSR (with the `cdylib` hosted as a release asset the loader fetches), on tagged release.
- **Versioning:** the binding artifacts track the workspace version (1.0.x); `version()`/`schemaVersion()` expose core's `schema_version` for runtime compatibility assertions.
- **Source fallback:** consumers without a matching prebuild build from source via `cargo` + `napi build` / `maturin build`.

### 3.11 Error Model

The binding preserves SPEC-1's two-level taxonomy across the boundary:

| Core variant | `code` (stable) | `exit_code` | Host surface |
|--------------|-----------------|-------------|--------------|
| `Config` | `config` | 2 | `AutoAgentError` |
| `Workspace` | `workspace` | 2 | `AutoAgentError` |
| `Analysis` | `analysis` | — | `AutoAgentError` |
| `Plan` | `plan` | 3 | `AutoAgentError` |
| `Policy(PathEscape)` | `policy.path_escape` | 4 | `AutoAgentError` |
| `Policy(BlockedCommand)` | `policy.blocked_command` | 4 | `AutoAgentError` |
| `Policy(WriteNotApproved)` | `policy.write_not_approved` | 4 | `AutoAgentError` (raised when approval refused, FR-7) |
| `Editing` | `editing` | 5 | `AutoAgentError` |
| `Validation` | `validation` | 6 | `AutoAgentError` |
| `Revert` | `revert` | 7 | `AutoAgentError` |
| `Memory` | `memory` | 8 | `AutoAgentError` |

Mapping is generated from `bind.rs` so new core variants surface automatically (FR-8). Host code branches on `err.code` / `err.exit_code`.

---

## 4. Implementation Plan

### 4.1 Build Phases

#### Phase 1: Registry + Generator Skeleton (B1)
- **Goal:** `bind.rs` declares the read-only surface (`init`, `doctor`, `analyze`, `version`); `main.rs generate` emits one backend (napi-rs) + `.d.ts` + `surface.schema.json`.
- **Scope:** workspace membership, Cargo.toml with feature scaffolding, registry format, codegen for one backend, error mapping for non-policy variants.
- **Exit criteria:** `cargo build -p autoagent-bingen --features node-napi` produces a `.node`; `bingen smoke` calls `doctor()` from a Node test; `bingen check` passes.

#### Phase 2: pyo3 + Full Read Surface (B2)
- **Goal:** Add pyo3 backend and `.pyi`; complete read/validate surface (`plan` read/validate, `memory` read, `config` read).
- **Scope:** `python/pyrs.rs` generation, abi3 wheel build via maturin, Python error subclass, `_sync` placeholders.
- **Exit criteria:** `import autoagent; autoagent.doctor(root)` works from a wheel built in CI on one platform; stubs type-check under `mypy`/`pyright`.

#### Phase 3: Mutating Surface + Safety Parity (B3)
- **Goal:** Bind `apply`, `run`, `evolve`, `revert`, `patch` with full PolicyEngine routing, snapshot/audit parity, and the approval-callback bridge (fail-closed).
- **Scope:** async bridging (Promise / asyncio + `_sync`), approval plumbing in `bind.rs`, panic containment, error mapping for policy variants.
- **Exit criteria:** a binding-initiated `run` produces an identical `.agent/runs/<id>/` trail to the CLI (asserted by a parity test); a privileged op with no approval **refuses**; `revert` undoes a binding run.

#### Phase 4: Deno Backend + Secondary Backends (B4)
- **Goal:** Generate and compile the Deno backends (`deno/deno_bindgen.rs` primary + `deno/ffi.rs` raw, with the `mod.ts` TS wrapper) and the secondary `node_bindgen.rs` (node-bindgen) and `python_bingen.rs` (RustPython); resolve the RustPython exposure mechanism (§8).
- **Scope:** four more generated adapters, the C-ABI `(ptr,len)`+free string helpers and `mod.ts` generation, feature gates, smoke coverage for all six backends.
- **Exit criteria:** all six backends build behind their features; `bingen smoke` exercises each (Deno via `deno run --allow-ffi`); surface parity holds across all six; a Deno `doctor()` returns the same object as the Node `doctor()`.

#### Phase 5: Distribution + CI Matrix (B5)
- **Goal:** Prebuilt npm (`@autoagent/native`) + PyPI wheels + Deno/JSR module across linux/macos/windows × x64/arm64, with loader shims and source fallback.
- **Scope:** package scaffolds generation (`package.json`, `pyproject.toml`, `deno.json`+`mod.ts`), CI matrix, publish workflow (npm/PyPI/JSR + `cdylib` release assets), checksums, drift guard in CI.
- **Exit criteria:** `npm install @autoagent/native`, `pip install autoagent`, and `deno run` against `jsr:@autoagent/native` (from a test index) load and run `doctor` on all 6 target cells; `bingen check` gates the release.

### 4.2 Testing Strategy

- **Unit (Rust):** registry parsing, marshaling round-trips (every boundary type), error-mapping table coverage (all `AutoAgentError` variants → correct `code`/`exit_code`).
- **Generation (golden):** `bingen generate` output compared against committed golden files; `bingen check` is the CI gate (FR-15).
- **Integration (per language):** Node (Jest/node:test), Python (pytest), and Deno (`deno test --allow-ffi`) suites calling each backend: `doctor`/`analyze` happy paths, `plan` validate failure → typed error, async `run` returns a `Promise`/awaitable.
- **Safety-parity (E2E):** **a real end-to-end test** — drive a full `run` from Node, from Python, *and* from Deno against a real temp workspace and a real `Autoagent.toml`, with no mocked core: assert the produced snapshot, patch, audit trail, and `revert` match the CLI's for the same plan. (Per project E2E definition: client → binding → core → filesystem, no mocked layers.)
- **Approval tests:** privileged op with (a) no callback → refuse, (b) callback returning `false` → refuse, (c) callback returning `true` / `approve:true` → proceed (including the Deno `Deno.UnsafeCallback` path).
- **Cross-backend equivalence:** same call on napi vs node-bindgen, pyo3 vs rustpython, and deno_bindgen vs raw FFI yields equal results.
- **Performance:** Criterion benches for call/marshaling overhead vs NFR targets.
- **Matrix:** all suites run across the 6-cell platform matrix in CI.

### 4.3 Rollout Strategy

- Pre-1.0 binding releases published under a `next`/pre-release tag on npm and PyPI; promote to `latest` after the safety-parity E2E passes on all 6 cells.
- Feature-gated backends let primary (napi-rs/pyo3) ship first; secondary (node-bindgen/rustpython) follow in a minor without breaking consumers.
- Rollback: unpublish/yank the pre-release tag; the CLI is unaffected (bindings are additive).

### 4.4 Operational Readiness

- Drift guard (`bingen check`) green in CI.
- Smoke harness green for every shipped backend.
- Safety-parity E2E green on all 6 cells.
- Published checksums and provenance headers verified.
- README sections for npm and PyPI install + a "bindings add no capability, only reach" safety note.

---

## 5. Milestones

| Milestone | Goal | Exit Criteria | Target Date | Owner |
|-----------|------|---------------|-------------|-------|
| B1 — Registry + napi-rs read surface | One-source registry + first backend | napi `.node` builds; `doctor()` from Node; `check` passes | TBD — owner to set | Ryan O'Boyle |
| B2 — pyo3 + full read surface | Python primary backend + read parity | abi3 wheel; `import autoagent` doctor works; stubs type-check | TBD | Ryan O'Boyle |
| B3 — Mutating surface + safety parity | apply/run/evolve/revert with no bypass | run-trail parity test vs CLI; fail-closed approval; revert works | TBD | Ryan O'Boyle |
| B4 — Deno + secondary backends | deno_bindgen + raw FFI + node-bindgen + RustPython | all 6 backends build + smoke; surface parity; Deno doctor == Node doctor | TBD | Ryan O'Boyle |
| B5 — Distribution + CI matrix | Prebuilt npm + PyPI + JSR, 6-cell matrix | install+run (npm/pip/deno) on all cells; `check` gates release | TBD | Ryan O'Boyle |

### Dependency Graph

```
B1 (registry + napi)
   │
   ├──► B2 (pyo3 + read surface)
   │        │
   └────────┴──► B3 (mutating + safety parity)
                      │
                      ├──► B4 (Deno + secondary backends)
                      │
                      └──► B5 (distribution + CI matrix)   ← also needs B4 for full matrix
```

---

## 6. Success Criteria

### 6.1 Launch Metrics

| Metric | Target | Measurement Method |
|--------|--------|--------------------|
| Backend coverage | 6/6 backends build + smoke green | CI `bingen smoke` per backend |
| Platform coverage | 6/6 cells publish + load (npm/PyPI/JSR) | CI matrix install test |
| Safety parity | 100% mutating ops via PolicyEngine; 0 bypass paths | parity E2E + code audit |
| Surface parity | `bingen check` clean (0 drift) | CI gate |
| Error fidelity | 100% variants mapped with correct `code`/`exit_code` | unit table test |
| Call overhead | within NFR targets *(provisional)* | Criterion benches |

### 6.2 Ongoing Monitoring

- CI dashboards: matrix build/test status, smoke results, bench trend lines, drift-check status.
- npm/PyPI: download counts and version adoption as a reach indicator.
- Issue triage cadence: weekly review of binding-tagged issues.

### 6.3 Remediation Triggers

- Any drift-check failure on `main` → block release, fix `bind.rs`/regenerate.
- Any safety-parity test failure → **stop ship** (treated as a safety regression, not a bug).
- Bench overhead > 2× target on any cell → investigate before publish.
- A platform cell failing to load on `latest` → yank and fix.

---

## 7. Risks

| ID | Risk | Impact | Likelihood | Mitigation | Contingency |
|----|------|--------|-----------|------------|-------------|
| R-1 | RustPython exposure model is immature/limited; `python_bingen` can't cleanly host the surface | Med | High | Treat RustPython as alternative, not primary; spike early (B4); keep pyo3 as the shipping Python path | Ship pyo3 only; mark RustPython experimental/feature-off |
| R-2 | Six backends drift from `bind.rs` despite generation (hand edits, partial regen) | High | Med | `bingen check` drift guard as a hard CI gate (FR-15); "do not edit" headers | Fail the build; regenerate from registry |
| R-3 | A binding path bypasses the PolicyEngine (direct core call skipping policy) | Critical | Low | Surface restricted to core's policy-routed workflow fns; audit + parity E2E; no raw-fs/exec binding (FR-19/20) | Stop ship; remove offending symbol |
| R-4 | Async bridge blocks host event loop or mishandles cancellation | Med | Med | Use napi `AsyncTask` / pyo3 `future_into_py`; concurrency test asserts non-blocking | Provide `_sync` only until fixed |
| R-5 | Rust panic at boundary aborts host process | High | Low | Catch-unwind wrappers convert panics to host errors; core uses `Result` | Wrap every export; add panic test |
| R-6 | Prebuilt matrix gaps (arm64/windows toolchain issues) leave consumers without a binary | Med | Med | cibuildwheel + napi-rs prebuild tooling; source fallback always available | Document source build; add cell later |
| R-7 | Backend crate license incompatibility with MIT | Low | Low | License audit in B1 (`cargo deny`) | Drop/replace offending backend |
| R-8 | Boundary type lacks `Serialize`/`Deserialize` (**confirmed**: `DoctorReport`/`Check`, `RunOutcome`/`RunState`, `AutoAgentError`), forcing core edits | Med | High | Additive `#[derive(Serialize, Deserialize)]` only, no logic change; done as a tracked precondition in B1/B2; CI asserts core behavior unchanged | Add a binding-local DTO mirror that maps from the core type |
| R-9 | NFR overhead targets unconfirmed; real overhead exceeds expectations | Low | Med | Benches from B1; targets provisional pending owner sign-off (§8) | Adjust targets or optimize marshaling |
| R-10 | Deno FFI is sync-by-default; exposing async `run`/`evolve` without blocking the Deno event loop is awkward over a C ABI | Med | Med | Use `Deno.dlopen` symbol `nonblocking: true` for async ops; bridge approval via `Deno.UnsafeCallback`; test non-blocking behavior | Offer Deno `*_sync` only until non-blocking validated |
| R-11 | Deno's `--allow-ffi` carries an "unstable/elevated trust" stigma; consumers wary of FFI permission | Low | Med | Document the permission model; note bindings add no capability beyond core (FR-19); ship the typed `mod.ts` that scopes FFI to this lib | Provide subprocess-CLI shim as a no-FFI fallback (future) |
| R-12 | `deno_bindgen` toolchain maturity / version churn breaks generated wrappers | Low | Med | Pin `deno_bindgen`; raw `ffi.rs` path is the dependency-free fallback that needs no external tool | Ship raw-FFI `mod.ts` only |

---

## 8. Open Questions

| # | Question | Owner | Due Date |
|---|----------|-------|----------|
| Q-1 | What is the exact RustPython exposure mechanism for `python_bingen` (native module vs embedded VM vs `freeze`)? Confirm it can host the full surface, or scope it down. | Ryan O'Boyle | Before B4 |
| Q-2 | Confirm the provisional NFR targets (≤2 ms call overhead, ≤10 ms marshaling, ≤150 ms cold load) or set authoritative numbers. | Ryan O'Boyle | Before B3 |
| Q-3 | Should `node_bindgen` and `napi` both publish to the **same** `@autoagent/native` package (selectable) or to separate packages? Likewise pyo3 vs rustpython on PyPI, and deno_bindgen vs raw-FFI on JSR. | Ryan O'Boyle | Before B5 |
| Q-8 | Deno distribution: JSR (`jsr:@autoagent/native`) vs `deno.land/x` vs npm-compat? And how is the `cdylib` delivered — release asset fetched at first load, or vendored? | Ryan O'Boyle | Before B5 |
| Q-9 | For Deno async ops, confirm `Deno.dlopen` `nonblocking: true` is acceptable (runs on a blocking threadpool) vs requiring a `*_sync`-only v1. | Ryan O'Boyle | Before B4 |
| Q-10 | What are the exact core entrypoints the `patch` command should bind (list/inspect/apply patch artifacts)? Confirm against the CLI's `patch` handler. | Ryan O'Boyle | Before B3 |
| Q-4 | Is JSON-at-the-boundary acceptable for v1, or do `analyze`/large results need native object mapping for the overhead target? | Ryan O'Boyle | Before B3 |
| Q-5 | Which Node and CPython minimum versions are committed (N-API 8 / abi3-py39 assumed)? | Ryan O'Boyle | Before B1 |
| Q-6 | Milestone target dates and whether any backend is deferrable past 1.0. | Ryan O'Boyle | Before B1 |
| Q-7 | Does the approval callback need to be `async` (host does I/O to decide), or is sync sufficient for v1? | Ryan O'Boyle | Before B3 |

---

## Appendices

### Appendix A: Glossary

| Term | Meaning |
|------|---------|
| **bingen** | This crate / its generator binary; reads `bind.rs` and emits all bindings. |
| **Surface / bound surface** | The curated subset of `autoagent-core` public functions exposed to Python/Node. |
| **Surface registry** | The declarative table in `bind.rs` that is the single source of truth. |
| **Backend** | One framework adapter producing a loadable module: napi-rs, node-bindgen, pyo3, RustPython, deno_bindgen, raw FFI. |
| **Deno FFI** | Deno's foreign-function interface; `Deno.dlopen` loads a C-ABI `cdylib` under `--allow-ffi`. |
| **deno_bindgen** | A crate + CLI that annotates Rust exports and generates the matching Deno TS (`mod.ts`) FFI wrapper. |
| **JSR** | The JavaScript Registry (`jsr.io`); Deno's first-class module registry. |
| **Stub** | A type-only file (`.d.ts` / `.pyi`) giving host IDEs autocomplete over the surface. |
| **Drift guard** | `bingen check`: fails CI if generated files differ from a fresh regeneration. |
| **Fail-closed approval** | A privileged op refuses unless explicitly approved (callback grant or `approve` flag). |
| **Safety parity** | The property that a binding-initiated op behaves identically to the CLI w.r.t. policy, snapshot, audit, revert. |
| **abi3** | The CPython stable ABI; one wheel works across Python ≥ 3.9. |
| **N-API** | Node's stable native addon ABI; `.node` binaries. |

### Appendix B: Backend Matrix

| Language | Primary | Alternative | Artifact |
|----------|---------|-------------|----------|
| Node.js | napi-rs (`napi.rs`) | node-bindgen (`node_bindgen.rs`) | `.node` (N-API) |
| Python | pyo3 (`pyrs.rs`) | RustPython (`python_bingen.rs`) — *experimental, not a `pip install` wheel; in-Rust interpreter embedding (R-1/Q-1)* | pyo3 → abi3 `.so`/`.pyd` wheel |
| Deno | deno_bindgen (`deno_bindgen.rs`) | raw FFI (`ffi.rs`) | C-ABI `cdylib` + `mod.ts` (loaded via `Deno.dlopen`) |

### Appendix C: Target Platform Matrix (FR-14)

| OS | x64 | arm64 |
|----|-----|-------|
| Linux | ✔ | ✔ |
| macOS | ✔ | ✔ |
| Windows | ✔ | ✔ |

### Appendix D: Generated File Inventory (from `bingen generate`)

```text
src/node/napi.rs              # napi-rs adapter        (generated)
src/node/node_bindgen.rs      # node-bindgen adapter   (generated)
src/python/pyrs.rs            # pyo3 adapter           (generated)
src/python/python_bingen.rs   # RustPython adapter     (generated)
src/deno/deno_bindgen.rs      # deno_bindgen adapter   (generated)
src/deno/ffi.rs               # raw C-ABI FFI adapter  (generated)
dist/index.d.ts               # TS stubs (Node + Deno)
dist/index.js                 # Node loader shim
package.json                  # npm scaffold (@autoagent/native)
python/autoagent/__init__.pyi # PEP 561 stubs
python/autoagent/__init__.py  # Python loader shim
pyproject.toml                # maturin/abi3 scaffold
deno/mod.ts                   # Deno FFI TS wrapper + loader
deno.json                     # Deno/JSR scaffold
schema/surface.schema.json    # machine-readable surface descriptor
```

### Appendix E: Decision Log

| # | Decision | Rationale |
|---|----------|-----------|
| D-1 | Generate backends from one registry rather than hand-write four | Eliminates backend drift; "add a command" = one edit (FR-1/FR-21). |
| D-2 | Ship all six backends, feature-gated | User directive; max toolchain compatibility; napi-rs/pyo3/deno_bindgen primary, others alternative. |
| D-3 | JSON-at-the-boundary marshaling (v1) | Reuses core's existing `serde` impls; native mapping is an optimization (Q-4). |
| D-4 | Bindings never weaken safety; approval fail-closed | SPEC-1 identity parity; bindings are transport, not authority (FR-6/7/19/20). |
| D-5 | Preserve `code`+`exit_code` taxonomy across the boundary | Lets host code branch on the same denial categories as the CLI (FR-8). |
| D-6 | RustPython is alternative, pyo3 primary | RustPython maturity risk (R-1); guarantees a shipping Python path. |
| D-7 | Deno added as a third target language with two backends | User directive; Deno is a first-class TS runtime whose FFI loads a C-ABI `cdylib` — same engine reach without a subprocess. |
| D-8 | deno_bindgen primary, raw `ffi.rs` alternative | deno_bindgen auto-generates the typed `mod.ts`; raw FFI is the dependency-free, toolchain-free fallback (R-12). |
| D-9 | Deno reuses the same `(ptr,len)` JSON-string ABI as the engine boundary | Keeps Deno on the one marshaling contract (D-3); only the transport (FFI vs N-API/pyo3) differs. |
