//! `bind::replay` reproduces a recorded session through the same policy-gated,
//! snapshotted apply path the original run used (the reproducible-autonomous-loop
//! contract). These tests pin: (1) fail-closed refusal without approval, (2)
//! bit-for-bit reproduction of the recorded change, (3) a clean structured error
//! for an unknown session rather than a panic.

use autoagent_bingen::bind;
use autoagent_core::config::config_schema::AutoAgentConfig;
use autoagent_core::planning::plan::Plan;
use autoagent_core::runtime::session;
use camino::Utf8Path;
use std::path::Path;

/// The one-step plan a recorded session replays: create `crates/x.rs` (under the
/// default include globs, so policy allows the write).
const PLAN_JSON: &str = r#"{"objective":"build x","summary":"s","files_to_read":[],
"files_to_create":[{"path":"crates/x.rs","purpose":"p"}],"files_to_modify":[],
"operations":[{"kind":"Create","path":"crates/x.rs","destination_path":null,"reason":"r",
  "before_hash":null,"after_hash":null,"content":"// x\n"}],
"validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#;

/// Init a temp workspace and record a one-step session; returns its id. The
/// recorded plan is *not* applied yet — replay is what reproduces it.
fn record_one_step_session(root: &str) -> String {
    bind::init(root).unwrap();
    let uroot = Utf8Path::from_path(Path::new(root)).unwrap();
    let cfg = AutoAgentConfig::load(uroot).unwrap();
    let plan: Plan = serde_json::from_str(PLAN_JSON).expect("valid plan json");
    session::record(uroot, &cfg, "build x", &[plan]).unwrap()
}

#[test]
fn replay_without_approval_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let id = record_one_step_session(root);

    let err = bind::replay(root, &id, false).unwrap_err();
    assert!(err.code.starts_with("policy"), "got {}", err.code);
    assert!(
        !Path::new(root).join("crates/x.rs").exists(),
        "a refused replay must not mutate the workspace"
    );
}

#[test]
fn replay_with_approval_reproduces_the_recorded_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let id = record_one_step_session(root);

    let json = bind::replay(root, &id, true).unwrap();
    assert!(json.contains("\"final_state\""), "outcome json: {json}");
    assert!(json.contains("Completed"), "replay should complete: {json}");
    // Bit-for-bit reproduction of the recorded operation's content.
    assert_eq!(
        std::fs::read_to_string(Path::new(root).join("crates/x.rs")).unwrap(),
        "// x\n",
        "replay must reproduce the recorded file content exactly"
    );
}

#[test]
fn replay_unknown_session_errors_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    bind::init(root).unwrap();

    // A session id that was never recorded must surface a structured BindError.
    let err = bind::replay(root, "20990101T000000Z-nope", true).unwrap_err();
    assert!(
        !err.code.is_empty(),
        "a missing session must surface an error, not panic"
    );
}
