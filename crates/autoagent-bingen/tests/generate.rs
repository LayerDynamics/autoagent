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
