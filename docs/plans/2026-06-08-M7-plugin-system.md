# M7 — 0.7.0 Plugin System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `lore:execute` to implement this plan task-by-task.
> **Scope guard:** Do ONLY what is listed here. If you discover adjacent issues, note them as a TODO and continue. Do NOT fix them.

**Goal:** Add an extensibility layer — Rust plugin traits, a tool registry, a plugin manifest, and WASM plugin support — where **every plugin routes through the same safety layer** (no privileged bypass).
**Architecture:** `autoagent-plugin-sdk` defines the `Plugin`/`Tool` traits and manifest schema. The core hosts a `ToolRegistry`; tools request file/command actions through the existing `PolicyEngine` rather than touching the filesystem directly. WASM plugins run in a sandboxed `wasmtime` runtime with host functions that also go through the policy engine.
**Tech Stack:** Rust 2021; **new deps** `wasmtime` (WASM host), `schemars` + `jsonschema` (manifest/tool schema validation) — all SPEC-1 §12 optional deps, introduced here.
**Practices:** TDD, typed-interfaces-first, contract-first.
**Required skills:** none. (WASM guest authoring is out of scope; this milestone ships the host + a sample native plugin + a sample WASM plugin loaded from a fixture.)
**Prerequisite:** **M1** (PolicyEngine, errors) — the safety layer plugins must route through. M2–M6 not required.
**Design status:** ⚠️ **PROPOSED DESIGN.** SPEC-1 §13 names "Rust plugin traits, WASM plugin support, tool registry, plugin manifest" and FR-24 fixes the invariant (all plugins through the safety layer). The trait shapes, manifest schema, registry API, and WASM host-function boundary below are design decisions to confirm. **The load-bearing constraint to verify: a plugin cannot perform any write or command that the PolicyEngine would reject.**

**Contracts introduced here (new):** `Plugin`, `Tool`, `ToolSchema`, `PluginManifest`, `ToolRegistry`, `HostContext`. These live in `autoagent-plugin-sdk` and are the stable plugin ABI from 1.0.0.

---

### Task 1: Plugin SDK crate + trait contracts (typed-first)

**Files:**
- Create: `crates/autoagent-plugin-sdk/Cargo.toml`
- Create: `crates/autoagent-plugin-sdk/src/lib.rs`
- Create: `crates/autoagent-plugin-sdk/src/plugin.rs`
- Create: `crates/autoagent-plugin-sdk/src/tool.rs`
- Create: `crates/autoagent-plugin-sdk/src/schema.rs`
- Modify: root `Cargo.toml` (add the crate to `members`)

**Step 1: Write the failing test** (`tool.rs`)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    struct Echo;
    impl Tool for Echo {
        fn name(&self) -> &str { "echo" }
        fn schema(&self) -> ToolSchema { ToolSchema::object(&[("text","string")]) }
        fn invoke(&self, input: serde_json::Value, _ctx: &mut dyn HostContext) -> ToolResult {
            Ok(serde_json::json!({"echoed": input["text"]}))
        }
    }
    #[test] fn tool_invokes() {
        let mut ctx = NullHost;
        let out = Echo.invoke(serde_json::json!({"text":"hi"}), &mut ctx).unwrap();
        assert_eq!(out["echoed"], "hi");
    }
}
```

**Step 2: Run to verify it fails** → `cargo test -p autoagent-plugin-sdk` → FAIL

**Step 3: Write minimal implementation**
```rust
// tool.rs
use serde_json::Value;
pub type ToolResult = Result<Value, String>;

/// All filesystem/command access by a tool MUST go through HostContext,
/// which routes to the core PolicyEngine. Tools never touch std::fs directly.
pub trait HostContext {
    fn write_file(&mut self, path: &str, content: &str) -> Result<(), String>;
    fn read_file(&mut self, path: &str) -> Result<String, String>;
    fn run_command(&mut self, command: &str) -> Result<String, String>;
}

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn schema(&self) -> ToolSchema;
    fn invoke(&self, input: Value, ctx: &mut dyn HostContext) -> ToolResult;
}
```
```rust
// plugin.rs
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;
    fn tools(&self) -> Vec<Box<dyn crate::tool::Tool>>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginManifest {
    pub name: String, pub version: String, pub api_version: u32,
    pub description: String, pub tools: Vec<String>,
}
```
`schema.rs` — `ToolSchema` wrapping a JSON Schema (`schemars`-derived or hand-built) with `ToolSchema::object(fields)` helper; `NullHost` test double in `#[cfg(test)]`.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(plugin-sdk): Plugin/Tool/HostContext trait contracts"`

---

### Task 2: Host context bridge (routes tool I/O through PolicyEngine)

**Files:**
- Create: `crates/autoagent-core/src/plugins/host_context.rs`
- Create: `crates/autoagent-core/src/plugins/mod.rs`
- Modify: `crates/autoagent-core/Cargo.toml` (depend on `autoagent-plugin-sdk`)

**Step 1: Write the failing test** (the safety-critical one — FR-24):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use autoagent_plugin_sdk::tool::HostContext;
    #[test] fn host_rejects_blocked_write() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("Autoagent.toml"), crate::config::default_config::default_toml()).unwrap();
        let mut host = CoreHost::new(root.to_path_buf(), engine_from(root));
        assert!(host.write_file(".git/config", "x").is_err());     // blocked path → tool write refused
        assert!(host.write_file("crates/ok.rs", "x").is_ok());     // allowed path
    }
    #[test] fn host_rejects_blocked_command() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let mut host = CoreHost::new(root.to_path_buf(), engine_from(root));
        assert!(host.run_command("sudo rm -rf /").is_err());
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `CoreHost { root, engine: PolicyEngine }` implementing the SDK `HostContext`: `write_file` calls `engine.check_write` then `FileEditor`; `read_file` calls `engine.check_read`; `run_command` calls `command_runner::run_one`. Any `PolicyError` is mapped to the SDK's `Result<_, String>`. **This is the single chokepoint guaranteeing FR-24** — there is no path from a tool to the filesystem that skips the engine.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(plugins): CoreHost routing tool I/O through PolicyEngine"`

---

### Task 3: Tool registry + manifest validation

**Files:**
- Create: `crates/autoagent-core/src/plugins/registry.rs`
- Create: `crates/autoagent-core/src/plugins/manifest.rs`
- Modify: `crates/autoagent-core/src/plugins/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn registers_and_invokes_native_tool() {
        let mut reg = ToolRegistry::new();
        reg.register_plugin(Box::new(SamplePlugin)).unwrap();  // SamplePlugin defined in test
        assert!(reg.has_tool("echo"));
        let mut host = null_host();
        let out = reg.invoke("echo", serde_json::json!({"text":"hi"}), &mut host).unwrap();
        assert_eq!(out["echoed"], "hi");
    }
    #[test] fn rejects_duplicate_tool_name() {
        let mut reg = ToolRegistry::new();
        reg.register_plugin(Box::new(SamplePlugin)).unwrap();
        assert!(reg.register_plugin(Box::new(SamplePlugin)).is_err());
    }
    #[test] fn rejects_incompatible_api_version() {
        let mut reg = ToolRegistry::new();
        assert!(reg.register_plugin(Box::new(FutureApiPlugin)).is_err());  // api_version > supported
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `ToolRegistry { tools: HashMap<String, Box<dyn Tool>> }`: `register_plugin` validates `manifest.api_version <= SUPPORTED_API_VERSION` (const), rejects duplicate tool names, inserts each tool; `invoke(name, input, host)` validates `input` against the tool's `ToolSchema` (via `jsonschema`) then calls `tool.invoke`. `manifest.rs` loads/validates a `plugin.toml` manifest from a plugin directory under `.agent/tools/`.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(plugins): tool registry with manifest + schema validation"`

---

### Task 4: WASM plugin host (sandboxed, host-fn through policy)

**Files:**
- Create: `crates/autoagent-core/src/plugins/wasm_host.rs`
- Modify: `crates/autoagent-core/src/plugins/mod.rs`
- Modify: `crates/autoagent-core/Cargo.toml` (add `wasmtime`)
- Create: `crates/autoagent-core/tests/fixtures/echo.wat` (a tiny WASM text fixture, compiled in-test)

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn loads_wasm_and_invokes_exported_tool() {
        // Load a minimal wasm module exporting `invoke(ptr,len)->(ptr,len)` that echoes input.
        let module_wat = include_str!("fixtures/echo.wat");
        let mut wasm = WasmPlugin::from_wat(module_wat).unwrap();
        let out = wasm.invoke_json(serde_json::json!({"text":"hi"})).unwrap();
        assert_eq!(out["echoed"], "hi");
    }
    #[test] fn wasm_host_write_goes_through_policy() {
        // a wasm module that calls host_write_file(".git/x") must get a denial back
        let mut wasm = WasmPlugin::from_wat(include_str!("fixtures/writer.wat")).unwrap();
        let res = wasm.invoke_json(serde_json::json!({"path":".git/x","content":"y"}));
        assert!(res.is_err());   // host function enforced PolicyEngine
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `WasmPlugin` wrapping a `wasmtime::Engine`/`Store`/`Instance`. Define host functions (`host_write_file`, `host_read_file`, `host_run_command`) imported by the guest; each delegates to `CoreHost` (Task 2) so the **same PolicyEngine** governs WASM tools. JSON crosses the boundary via linear-memory ptr/len marshalling. The store's `WASI`/capabilities are NOT granted filesystem access — the only I/O path is the policy-checked host functions. Memory/fuel limits set to bound runaway guests.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(plugins): wasmtime host with policy-gated host functions"`

---

### Task 5: Sample plugin + `plugin`/`tools` discovery wiring + E2E

**Files:**
- Create: `crates/autoagent-core/src/plugins/sample.rs` (a native sample plugin used by tests/docs)
- Modify: `crates/autoagent-cli/src/main.rs` (add `Tools { #[command(subcommand)] sub: ToolsSub }` with `List`)
- Create: `crates/autoagent-cli/src/commands/tools.rs`
- Create: `crates/autoagent-cli/tests/e2e_tools.rs`

**Step 1: Write the failing E2E**
```rust
use std::process::Command;
fn bin() -> &'static str { env!("CARGO_BIN_EXE_autoagent") }
#[test] fn tools_list_shows_builtin_sample() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new(bin()).args(["--yes","init"]).current_dir(root).output().unwrap();
    let out = Command::new(bin()).args(["tools","list"]).current_dir(root).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("echo"));
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — register the native sample plugin into the registry at startup; `tools list` prints registered tool names + their plugin + api_version. Discovery of WASM plugins scans `.agent/tools/*/plugin.toml` and loads each via `WasmPlugin` (errors logged, never crash the CLI).

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(cli): tools list + plugin discovery + e2e"`

---

### Task 6: Quality gate + M7 exit

**Step 1:** fmt + clippy + `cargo test --workspace` green.
**Step 2: Verify M7 exit criteria (SPEC-1 §5):** a sample plugin registers and runs entirely through the safety layer — assert via Task 2 + Task 4 tests that a plugin write to a blocked path is refused by the PolicyEngine, native AND WASM.
**Step 3: Commit** → `git add -A && git commit -m "chore(0.7.0): plugin system milestone exit"`

---

## Open design questions (resolve during execution)
- Manifest format: `plugin.toml` vs JSON manifest (current: TOML, consistent with `Autoagent.toml`).
- WASM ABI: ptr/len JSON marshalling vs the WASM Component Model / WIT (current: simple ptr/len for 0.7.0; Component Model is a 1.x consideration).
- Whether native (dylib) third-party plugins are allowed or only first-party + WASM (PROPOSED: only first-party native + sandboxed WASM third-party, since a third-party dylib could bypass the host boundary — a real safety concern worth confirming).
