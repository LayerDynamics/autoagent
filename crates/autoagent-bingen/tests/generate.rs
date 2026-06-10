//! The generator turns the surface registry into backend source, type stubs,
//! and a JSON schema. These tests pin the generated content (golden-checked by
//! the drift guard at runtime via `bingen check`).

use autoagent_bingen::gen;

#[test]
fn generate_emits_napi_and_dts_and_schema() {
    let out = gen::render_all(); // pure: path -> content, no fs writes
    let napi = out.get("src/node/napi.rs").expect("napi backend emitted");
    assert!(napi.contains("#[napi"));
    assert!(napi.contains("DO NOT EDIT"));

    let dts = out.get("dist/index.d.ts").expect("d.ts emitted");
    assert!(dts.contains("export function doctor"));

    let schema = out
        .get("schema/surface.schema.json")
        .expect("schema emitted");
    assert!(schema.contains("\"name\": \"run\""));
    assert!(schema.contains("\"privilege\": \"mutate\""));
}

#[test]
fn generate_emits_pyo3_and_pyi() {
    let out = gen::render_all();
    let py = out.get("src/python/pyrs.rs").expect("pyo3 backend emitted");
    assert!(py.contains("#[pyfunction]"));
    assert!(py.contains("#[pymodule]"));
    assert!(py.contains("create_exception!"));

    let pyi = out
        .get("python/autoagent/__init__.pyi")
        .expect("pyi emitted");
    assert!(pyi.contains("def doctor"));
    assert!(pyi.contains("def version"));
}

#[test]
fn ffi_backend_emits_c_abi() {
    let out = gen::render_all();
    let ffi = out.get("src/deno/ffi.rs").expect("ffi backend emitted");
    assert!(ffi.contains("#[no_mangle]"));
    assert!(ffi.contains("extern \"C\""));
    assert!(ffi.contains("pub unsafe extern \"C\" fn aa_free"));
    assert!(ffi.contains("pub unsafe extern \"C\" fn aa_doctor"));
    assert!(ffi.contains("pub unsafe extern \"C\" fn aa_apply"));

    let modts = out.get("deno/mod.ts").expect("deno mod.ts emitted");
    assert!(modts.contains("Deno.dlopen"));
    assert!(modts.contains("export function doctor"));
    assert!(modts.contains("export function apply"));
    assert!(modts.contains("export class AutoAgentError"));

    let rp = out
        .get("src/python/python_bingen.rs")
        .expect("rustpython backend emitted");
    assert!(rp.contains("#[rustpython_vm::pymodule]"));
    assert!(rp.contains("#[pyfunction]"));
    assert!(rp.contains("pub fn module_def"));

    let nb = out
        .get("src/node/node_bindgen.rs")
        .expect("node-bindgen backend emitted");
    assert!(nb.contains("#[node_bindgen"));
    assert!(nb.contains("napi_register_module_v1"));

    let db = out
        .get("src/deno/deno_bindgen.rs")
        .expect("deno_bindgen backend emitted");
    assert!(db.contains("#[deno_bindgen]"));
    assert!(db.contains("pub fn aa_doctor(root: &str) -> String"));
    // booleans are u8 (deno_bindgen 0.8 has no bool param type).
    assert!(db.contains("approve: u8"));
}

#[test]
fn mutating_sync_surface_emitted() {
    let out = gen::render_all();
    let napi = out["src/node/napi.rs"].as_str();
    let py = out["src/python/pyrs.rs"].as_str();
    for f in ["apply", "revert", "run_sync", "evolve_sync"] {
        assert!(py.contains(&format!("fn {f}(")), "pyo3 missing {f}");
    }
    assert!(napi.contains("pub fn apply"));
    assert!(napi.contains("pub fn revert"));
    assert!(napi.contains("pub fn run_sync"));
    // The async run/evolve must NOT be emitted yet (unwired — no lying stubs).
    assert!(!out["dist/index.d.ts"].contains("export function run("));
}

#[test]
fn package_scaffolds_emitted() {
    let out = gen::render_all();
    assert!(out["package.json"].contains("@autoagent/native"));
    assert!(out["package.json"].contains("\"binaryName\": \"autoagent\""));
    assert!(out["deno.json"].contains("./deno/mod.ts"));
    assert!(out["dist/index.js"].contains("autoagent.${triple()}.node"));
}

#[test]
fn wired_read_surface_present_in_all_stub_dialects() {
    let out = gen::render_all();
    // The read surface added in B2-T1 must appear in both stub dialects.
    for sym in ["configShow", "patchList", "memoryShow", "toolsList"] {
        assert!(
            out["dist/index.d.ts"].contains(sym),
            "{sym} missing from .d.ts"
        );
    }
    for sym in ["config_show", "patch_list", "memory_show", "tools_list"] {
        assert!(
            out["python/autoagent/__init__.pyi"].contains(sym),
            "{sym} missing from .pyi"
        );
    }
}
