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
