//! E2E (SPEC-1 §4.2): the real binary lists the built-in sample tool.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_autoagent")
}

#[test]
fn tools_list_shows_builtin_sample() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();

    let out = Command::new(bin())
        .args(["tools", "list"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("echo"));
}
