//! E2E (SPEC-1 §4.2): the real binary runs a supervised workflow from a plan,
//! applying a file change AND executing a real validation command through the
//! command guard, then writing the validation report. No mocked layers.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_autoagent")
}

#[test]
fn run_applies_then_real_validation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();
    // Real git repo so the allowed `git status` validation command succeeds.
    Command::new("git")
        .arg("init")
        .current_dir(root)
        .output()
        .unwrap();

    std::fs::write(
        root.join("p.plan.json"),
        r#"{"objective":"touch","summary":"s","files_to_read":[],
      "files_to_create":[{"path":"crates/ok.rs","purpose":"x"}],"files_to_modify":[],
      "operations":[{"kind":"Create","path":"crates/ok.rs","destination_path":null,"reason":"r",
        "before_hash":null,"after_hash":null,"content":"pub fn ok() -> i32 { 1 }\n"}],
      "validation_commands":["git status"],"risks":[],"rollback_strategy":"snapshot"}"#,
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["--yes", "run", "--from", "p.plan.json", "touch"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(root.join("crates/ok.rs").exists());

    let run_dir = std::fs::read_dir(root.join(".agent/runs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(run_dir.join("validation-report.md").exists());
    assert!(run_dir.join("summary.md").exists());
    let report = std::fs::read_to_string(run_dir.join("validation-report.md")).unwrap();
    assert!(report.contains("PASSED"));
}
