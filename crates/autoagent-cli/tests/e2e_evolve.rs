//! E2E (SPEC-1 §4.2): the real binary proves the marquee safety property —
//! `evolve --apply` does NOT modify source when `allow_self_modification` is
//! false (the default), even when `--apply` is passed.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_autoagent")
}

#[test]
fn evolve_apply_refused_when_self_mod_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();
    // default config: allow_self_modification = false
    std::fs::create_dir_all(root.join("crates")).unwrap();
    std::fs::write(root.join("crates/keep.rs"), "ORIGINAL").unwrap();
    std::fs::write(
        root.join("p.plan.json"),
        r#"{"objective":"self","summary":"s","files_to_read":[],
      "files_to_create":[],"files_to_modify":[{"path":"crates/keep.rs","purpose":"x"}],
      "operations":[{"kind":"Replace","path":"crates/keep.rs","destination_path":null,"reason":"r",
        "before_hash":null,"after_hash":null,"content":"HACKED"}],
      "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["evolve", "--from", "p.plan.json", "--apply", "self"])
        .current_dir(root)
        .output()
        .unwrap();

    // Apply refused → non-zero exit (policy), and the source is UNCHANGED.
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(
        std::fs::read_to_string(root.join("crates/keep.rs")).unwrap(),
        "ORIGINAL"
    );
}

#[test]
fn evolve_plan_only_writes_plan() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();
    std::fs::write(
        root.join("p.plan.json"),
        r#"{"objective":"self","summary":"s","files_to_read":[],
      "files_to_create":[{"path":"crates/new.rs","purpose":"x"}],"files_to_modify":[],
      "operations":[{"kind":"Create","path":"crates/new.rs","destination_path":null,"reason":"r",
        "before_hash":null,"after_hash":null,"content":"// x"}],
      "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["evolve", "--from", "p.plan.json", "self"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // plan-only: source NOT created
    assert!(!root.join("crates/new.rs").exists());
    // a plan artifact WAS written
    let plans: Vec<_> = std::fs::read_dir(root.join(".agent/plans"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!plans.is_empty());
}
