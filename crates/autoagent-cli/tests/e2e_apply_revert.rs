//! Genuine end-to-end test (SPEC-1 §4.2): invokes the compiled `autoagent`
//! binary as a subprocess against a real throwaway workspace on the real
//! filesystem — no mocked layers. M1 has no LLM, so nothing is stubbed.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_autoagent")
}

#[test]
fn init_apply_revert_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // init
    let out = Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(root.join("Autoagent.toml").exists());

    // seed a tracked file + a plan that edits it
    std::fs::create_dir_all(root.join("crates")).unwrap();
    std::fs::write(root.join("crates/a.rs"), "ORIGINAL").unwrap();
    std::fs::write(
        root.join("p.plan.json"),
        r#"{"objective":"edit a","summary":"s",
      "files_to_read":[],"files_to_create":[],"files_to_modify":[{"path":"crates/a.rs","purpose":"x"}],
      "operations":[{"kind":"Replace","path":"crates/a.rs","destination_path":null,"reason":"r",
        "before_hash":null,"after_hash":null,"content":"CHANGED"}],
      "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
    )
    .unwrap();

    // apply
    let out = Command::new(bin())
        .args(["--yes", "apply", "p.plan.json"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("crates/a.rs")).unwrap(),
        "CHANGED"
    );

    // discover the run id and revert
    let runs_dir = root.join(".agent/runs");
    let run_id = std::fs::read_dir(&runs_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name();
    let out = Command::new(bin())
        .args(["--yes", "revert", run_id.to_str().unwrap()])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "revert failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("crates/a.rs")).unwrap(),
        "ORIGINAL"
    );
}

#[test]
fn apply_to_blocked_path_is_refused_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();

    // A plan trying to write inside .git/ must be refused by the policy engine.
    std::fs::write(
        root.join("evil.plan.json"),
        r#"{"objective":"escape","summary":"s","files_to_read":[],
      "files_to_create":[],"files_to_modify":[],
      "operations":[{"kind":"Write","path":".git/hooks/pre-commit","destination_path":null,
        "reason":"r","before_hash":null,"after_hash":null,"content":"evil"}],
      "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["--yes", "apply", "evil.plan.json"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(!out.status.success(), "blocked-path apply should fail");
    assert_eq!(out.status.code(), Some(4), "policy errors exit with code 4");
    assert!(!root.join(".git/hooks/pre-commit").exists());
}
