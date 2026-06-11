//! E2E (SPEC-1 §4.2): `autoagent run --replay <id>` reproduces a recorded session
//! through the REAL binary — no model, no mocked layers. A one-step session is
//! laid down in the on-disk format the engine records, then replayed; the binary
//! re-applies the plan deterministically and the recorded change reappears.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_autoagent")
}

const PLAN: &str = r#"{"objective":"build x","summary":"s","files_to_read":[],
"files_to_create":[{"path":"crates/x.rs","purpose":"p"}],"files_to_modify":[],
"operations":[{"kind":"Create","path":"crates/x.rs","destination_path":null,"reason":"r",
  "before_hash":null,"after_hash":null,"content":"pub fn x() -> i32 { 1 }\n"}],
"validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#;

/// Init via the real binary, then lay down a one-step recorded session on disk.
fn seed_session(root: &std::path::Path, id: &str) {
    let init = Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(init.status.success(), "init failed");

    let sdir = root.join(".agent/sessions").join(id);
    std::fs::create_dir_all(&sdir).unwrap();
    std::fs::write(
        sdir.join("session.json"),
        format!(
            r#"{{"session_id":"{id}","objective":"build x","created":"20200101T000000Z","steps":1}}"#
        ),
    )
    .unwrap();
    std::fs::write(sdir.join("step-001.plan.json"), PLAN).unwrap();
}

#[test]
fn run_replay_reproduces_recorded_session() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let id = "20200101T000000Z-build-x";
    seed_session(root, id);

    // The target does not exist yet — replay is what creates it.
    assert!(!root.join("crates/x.rs").exists());

    let out = Command::new(bin())
        .args(["--yes", "run", "--replay", id])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "replay failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The recorded change is reproduced byte-for-byte.
    assert_eq!(
        std::fs::read_to_string(root.join("crates/x.rs")).unwrap(),
        "pub fn x() -> i32 { 1 }\n"
    );
    // The run is recorded as a reversible run folder, like any other.
    assert!(root.join(".agent/runs").is_dir());
}

#[test]
fn run_replay_unknown_session_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();

    let out = Command::new(bin())
        .args(["--yes", "run", "--replay", "no-such-session"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "replaying an unknown session must exit non-zero"
    );
}
