//! Cross-backend equivalence: all six backends + the TS wrapper are generated
//! from one WIRED surface, so they MUST expose the same symbol set, and the
//! neutral wrappers (the single source of every backend's behavior) must be
//! deterministic. This guards against a backend silently drifting from parity.

use autoagent_bingen::bind::{Privilege, SURFACE};
use autoagent_bingen::gen;

/// The read symbols every backend exposes (camelCase variant for JS/TS where
/// applicable is checked separately).
fn wired_read_names() -> Vec<&'static str> {
    SURFACE
        .iter()
        .filter(|s| matches!(s.privilege, Privilege::Read))
        .map(|s| s.name)
        .collect()
}

#[test]
fn every_backend_exposes_the_same_read_surface() {
    let out = gen::render_all();
    let rust_backends = [
        "src/node/napi.rs",
        "src/node/node_bindgen.rs",
        "src/python/pyrs.rs",
        "src/python/python_bingen.rs",
        "src/deno/ffi.rs",
        "src/deno/deno_bindgen.rs",
    ];
    for name in wired_read_names() {
        for backend in rust_backends {
            let src = out.get(backend).unwrap_or_else(|| panic!("missing {backend}"));
            assert!(
                src.contains(name),
                "backend {backend} is missing read symbol `{name}`"
            );
        }
    }
}

#[test]
fn deno_ts_and_node_dts_share_camel_surface() {
    let out = gen::render_all();
    let dts = &out["dist/index.d.ts"];
    let modts = &out["deno/mod.ts"];
    for cam in ["doctor", "configShow", "patchList", "memoryShow", "toolsList"] {
        assert!(dts.contains(cam), ".d.ts missing {cam}");
        assert!(modts.contains(cam), "mod.ts missing {cam}");
    }
}

#[test]
fn neutral_wrapper_is_deterministic() {
    // Every backend funnels through the same neutral wrapper, so determinism
    // here means cross-backend result equivalence.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let a = autoagent_bingen::bind::doctor(root).unwrap();
    let b = autoagent_bingen::bind::doctor(root).unwrap();
    assert_eq!(a, b);
    assert!(a.contains("\"checks\""));
}
