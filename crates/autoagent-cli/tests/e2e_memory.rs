//! E2E (SPEC-1 §4.2): the real binary rebuilds project memory from a real Rust
//! project and shows it back. No mocked layers.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_autoagent")
}

#[test]
fn memory_rebuild_then_show() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"demo\"\nversion=\"0.1.0\"",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "fn a(){}").unwrap();

    let rebuild = Command::new(bin())
        .args(["memory", "rebuild"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(rebuild.status.success());
    assert!(root.join(".agent/memory/project.json").exists());

    let show = Command::new(bin())
        .args(["memory", "show"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(show.status.success());
    assert!(String::from_utf8_lossy(&show.stdout).contains("demo"));
}

#[test]
fn memory_add_and_remove_decision() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new(bin())
        .args(["--yes", "init"])
        .current_dir(root)
        .output()
        .unwrap();

    let add = Command::new(bin())
        .args(["memory", "add", "use TOML config", "safer than JS"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(add.status.success());

    let show = Command::new(bin())
        .args(["memory", "show"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&show.stdout).contains("use TOML config"));

    let remove = Command::new(bin())
        .args(["memory", "remove", "d-1"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&remove.stdout).contains("removed decision d-1"));
}
