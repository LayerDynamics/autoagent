//! E2E (SPEC-1 §4.2): the real binary analyzes a real Rust project and writes
//! the project-analysis report. No mocked layers.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_autoagent")
}

#[test]
fn analyze_writes_report() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\n[dependencies]\nserde=\"1\"",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "fn a(){}").unwrap();

    let out = Command::new(bin())
        .args(["analyze"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "analyze failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let report = root.join(".agent/reports/project-analysis.md");
    assert!(report.exists());
    let md = std::fs::read_to_string(&report).unwrap();
    assert!(md.contains("Rust"));
    assert!(md.contains("serde"));
}
