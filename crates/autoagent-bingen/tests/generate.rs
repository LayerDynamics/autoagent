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
    let py = out
        .get("src/python/pyrs.rs")
        .expect("pyo3 backend emitted");
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
