//! E2E (SPEC-1 §4.2): the real binary imports a JSON plan via `--from`,
//! validates it, and writes the paired plan artifacts. The LLM-generate path is
//! covered by core unit tests with a fake provider; E2E avoids a live model.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_autoagent")
}

#[test]
fn plan_import_writes_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();
    std::fs::write(
        root.join("in.plan.json"),
        r#"{"objective":"o","summary":"s","files_to_read":[],
      "files_to_create":[],"files_to_modify":[],"operations":[{"kind":"Create","path":"crates/x.rs",
      "destination_path":null,"reason":"r","before_hash":null,"after_hash":null,"content":"x"}],
      "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["plan", "--from", "in.plan.json", "imported objective"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "plan failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let plans: Vec<_> = std::fs::read_dir(root.join(".agent/plans"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(plans
        .iter()
        .any(|e| e.file_name().to_string_lossy().ends_with(".plan.json")));
    assert!(plans
        .iter()
        .any(|e| e.file_name().to_string_lossy().ends_with(".plan.md")));
}

#[test]
fn plan_import_of_blocked_path_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();
    std::fs::write(
        root.join("bad.plan.json"),
        r#"{"objective":"o","summary":"s","files_to_read":[],
      "files_to_create":[],"files_to_modify":[],"operations":[{"kind":"Write","path":"target/x",
      "destination_path":null,"reason":"r","before_hash":null,"after_hash":null,"content":"x"}],
      "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
    )
    .unwrap();
    let out = Command::new(bin())
        .args(["plan", "--from", "bad.plan.json", "x"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(4));
}
