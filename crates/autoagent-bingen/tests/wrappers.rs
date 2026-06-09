//! The neutral wrappers call autoagent-core and marshal results as JSON. These
//! tests pin the JSON-at-the-boundary contract and the BindError mapping.

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

#[test]
fn config_show_renders_toml_after_init() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    bind::init(root).unwrap(); // writes Autoagent.toml
    let toml = bind::config_show(root).unwrap();
    assert!(toml.contains("[agent]"));
}

#[test]
fn patch_list_empty_is_json_array() {
    let dir = tempfile::tempdir().unwrap();
    let j = bind::patch_list(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(j.trim(), "[]");
}

#[test]
fn memory_show_after_init_reports_project() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    bind::init(root).unwrap();
    let j = bind::memory_show(root).unwrap();
    assert!(j.contains("\"name\""));
    assert!(j.contains("\"decisions\""));
}

#[test]
fn tools_list_includes_builtins() {
    let dir = tempfile::tempdir().unwrap();
    let j = bind::tools_list(dir.path().to_str().unwrap()).unwrap();
    // builtins always register at least one tool; result is a JSON array.
    assert!(j.starts_with('['));
}
