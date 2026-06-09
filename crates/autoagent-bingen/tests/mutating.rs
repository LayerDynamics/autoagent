//! Mutating wrappers route through core's policy engine + snapshot + audit, via
//! the CallbackGate. These tests use the `--from` (no-LLM) path with a real temp
//! workspace; they prove apply/revert round-trip and fail-closed refusal.

use autoagent_bingen::bind;
use std::path::Path;

/// Seed a temp workspace: init it, write a policy-valid plan that creates
/// `crates/x.rs` (matches the default include globs). Returns the plan path.
fn seed(root: &str) -> String {
    bind::init(root).unwrap();
    let plan = format!("{root}/p.json");
    std::fs::write(
        &plan,
        r#"{"objective":"contract","summary":"s","files_to_read":[],
        "files_to_create":[{"path":"crates/x.rs","purpose":"p"}],"files_to_modify":[],
        "operations":[{"kind":"Create","path":"crates/x.rs","destination_path":null,"reason":"r",
          "before_hash":null,"after_hash":null,"content":"// x"}],
        "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
    )
    .unwrap();
    plan
}

#[test]
fn apply_with_approval_then_revert_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let plan = seed(root);

    let run_id = bind::apply(root, &plan, true).unwrap();
    assert!(!run_id.is_empty());
    assert!(Path::new(root).join("crates/x.rs").exists());

    bind::revert(root, &run_id).unwrap();
    assert!(
        !Path::new(root).join("crates/x.rs").exists(),
        "revert should remove the created file"
    );
}

#[test]
fn apply_without_approval_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let plan = seed(root);

    let err = bind::apply(root, &plan, false).unwrap_err();
    assert!(err.code.starts_with("policy"), "got {}", err.code);
    assert!(
        !Path::new(root).join("crates/x.rs").exists(),
        "a refused apply must not mutate the workspace"
    );
}

#[test]
fn run_from_plan_with_approval_completes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let plan = seed(root);

    let j = bind::run_sync(root, "contract", Some(&plan), true).unwrap();
    assert!(j.contains("run_id"));
    assert!(j.contains("final_state"));
}

#[test]
fn run_without_approval_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let plan = seed(root);

    let err = bind::run_sync(root, "contract", Some(&plan), false).unwrap_err();
    assert!(err.code.starts_with("policy"), "got {}", err.code);
}
