//! WASM plugin host (M7) — runs sandboxed WebAssembly plugins on `wasmtime`.
//! The guest has NO direct I/O; its only host imports (`host.write_file`, etc.)
//! are routed through `CoreHost` → PolicyEngine, so a WASM tool is bound by the
//! exact same safety layer as a native one (SPEC-1 FR-24). Execution is
//! fuel-limited to bound runaway guests, and no WASI filesystem is granted.

use crate::error::{AutoAgentError, Result};
use crate::plugins::host_context::CoreHost;
use crate::safety::policy_engine::PolicyEngine;
use autoagent_plugin_sdk::tool::HostContext;
use camino::Utf8PathBuf;
use wasmtime::{Caller, Engine, Extern, Linker, Module, Store};

const FUEL: u64 = 5_000_000;

pub struct WasmPlugin {
    engine: Engine,
    module: Module,
}

impl WasmPlugin {
    pub fn from_wat(wat: &str) -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config).map_err(|e| AutoAgentError::Plugin(e.to_string()))?;
        let module =
            Module::new(&engine, wat).map_err(|e| AutoAgentError::Plugin(e.to_string()))?;
        Ok(Self { engine, module })
    }

    /// Call an exported `(func (param i32) (result i32))` with no host imports —
    /// proves real sandboxed WASM execution.
    pub fn call_unary(&self, name: &str, arg: i32) -> Result<i32> {
        let mut store = Store::new(&self.engine, ());
        store
            .set_fuel(FUEL)
            .map_err(|e| AutoAgentError::Plugin(e.to_string()))?;
        let linker: Linker<()> = Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| AutoAgentError::Plugin(e.to_string()))?;
        let func = instance
            .get_typed_func::<i32, i32>(&mut store, name)
            .map_err(|e| AutoAgentError::Plugin(e.to_string()))?;
        func.call(&mut store, arg)
            .map_err(|e| AutoAgentError::Plugin(e.to_string()))
    }

    /// Instantiate with a policy-routed `host.write_file` import and call the
    /// exported `(func (export "run") (result i32))`. The guest's write attempt
    /// returns 0 when the PolicyEngine allowed it, 1 when policy denied it.
    pub fn run_with_policy(&self, root: Utf8PathBuf, policy: PolicyEngine) -> Result<i32> {
        let mut store = Store::new(&self.engine, CoreHost::new(root, policy));
        store
            .set_fuel(FUEL)
            .map_err(|e| AutoAgentError::Plugin(e.to_string()))?;

        let mut linker: Linker<CoreHost> = Linker::new(&self.engine);
        linker
            .func_wrap(
                "host",
                "write_file",
                |mut caller: Caller<'_, CoreHost>,
                 path_ptr: i32,
                 path_len: i32,
                 content_ptr: i32,
                 content_len: i32|
                 -> i32 {
                    let mem = match caller.get_export("memory").and_then(Extern::into_memory) {
                        Some(m) => m,
                        None => return 2,
                    };
                    let data = mem.data(&caller);
                    let path = read_str(data, path_ptr, path_len);
                    let content = read_str(data, content_ptr, content_len);
                    // `data` borrow ends here; now we can take &mut to the host.
                    match caller.data_mut().write_file(&path, &content) {
                        Ok(()) => 0,
                        Err(_) => 1,
                    }
                },
            )
            .map_err(|e| AutoAgentError::Plugin(e.to_string()))?;

        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| AutoAgentError::Plugin(e.to_string()))?;
        let func = instance
            .get_typed_func::<(), i32>(&mut store, "run")
            .map_err(|e| AutoAgentError::Plugin(e.to_string()))?;
        func.call(&mut store, ())
            .map_err(|e| AutoAgentError::Plugin(e.to_string()))
    }
}

fn read_str(data: &[u8], ptr: i32, len: i32) -> String {
    let (p, l) = (ptr.max(0) as usize, len.max(0) as usize);
    if p.saturating_add(l) <= data.len() {
        String::from_utf8_lossy(&data[p..p + l]).into_owned()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::config_schema::AutoAgentConfig;
    use crate::config::default_config;

    fn policy_for(root: &camino::Utf8Path) -> (Utf8PathBuf, PolicyEngine) {
        let real =
            camino::Utf8PathBuf::from_path_buf(std::fs::canonicalize(root.as_std_path()).unwrap())
                .unwrap();
        let cfg = AutoAgentConfig::from_toml_str(&default_config::default_toml()).unwrap();
        let engine = PolicyEngine::from_config(&cfg, real.clone());
        (real, engine)
    }

    #[test]
    fn loads_wasm_and_runs_unary() {
        let wat = r#"(module
            (func (export "add_one") (param i32) (result i32)
                (i32.add (local.get 0) (i32.const 1))))"#;
        let p = WasmPlugin::from_wat(wat).unwrap();
        assert_eq!(p.call_unary("add_one", 41).unwrap(), 42);
    }

    #[test]
    fn wasm_host_write_to_git_is_denied_by_policy() {
        // path ".git/config" (11 bytes) at offset 0, content "x" (1 byte) at 32.
        let wat = r#"(module
            (import "host" "write_file" (func $w (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) ".git/config")
            (data (i32.const 32) "x")
            (func (export "run") (result i32)
                (call $w (i32.const 0) (i32.const 11) (i32.const 32) (i32.const 1))))"#;
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let (real, engine) = policy_for(root);
        let p = WasmPlugin::from_wat(wat).unwrap();
        assert_eq!(p.run_with_policy(real, engine).unwrap(), 1); // denied
    }

    #[test]
    fn wasm_host_write_to_allowed_path_succeeds() {
        // path "crates/ok.rs" (12 bytes) at 0, content "y" (1 byte) at 32.
        let wat = r#"(module
            (import "host" "write_file" (func $w (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "crates/ok.rs")
            (data (i32.const 32) "y")
            (func (export "run") (result i32)
                (call $w (i32.const 0) (i32.const 12) (i32.const 32) (i32.const 1))))"#;
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let (real, engine) = policy_for(root);
        let p = WasmPlugin::from_wat(wat).unwrap();
        assert_eq!(p.run_with_policy(real.clone(), engine).unwrap(), 0); // allowed
        assert_eq!(
            std::fs::read_to_string(real.join("crates/ok.rs").as_std_path()).unwrap(),
            "y"
        );
    }
}
