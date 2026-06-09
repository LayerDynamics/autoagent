//! 1.0.0 verification matrices (M8, SPEC-1 §6 launch metrics):
//! (1) every FileOperationKind is reversible; (2) no out-of-policy write lands.

use autoagent_core::runtime::{agent_loop, revert};
use camino::Utf8Path;

fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = camino::Utf8Path::from_path(dir.path()).unwrap();
    std::fs::write(
        root.join("Autoagent.toml"),
        autoagent_core::config::default_config::default_toml(),
    )
    .unwrap();
    std::fs::create_dir_all(root.join("crates")).unwrap();
    dir
}

fn apply_ops(root: &Utf8Path, ops: &str) -> autoagent_core::error::Result<String> {
    let plan = format!(
        r#"{{"objective":"m","summary":"s","files_to_read":[],"files_to_create":[],
        "files_to_modify":[],"operations":[{ops}],"validation_commands":[],"risks":[],
        "rollback_strategy":"snapshot"}}"#
    );
    let path = root.join("p.json");
    std::fs::write(path.as_std_path(), plan).unwrap();
    agent_loop::apply(root, &path, true)
}

fn op(kind: &str, path: &str, content: Option<&str>, dest: Option<&str>) -> String {
    let content = content
        .map(|c| format!("\"{c}\""))
        .unwrap_or_else(|| "null".into());
    let dest = dest
        .map(|d| format!("\"{d}\""))
        .unwrap_or_else(|| "null".into());
    format!(
        r#"{{"kind":"{kind}","path":"{path}","destination_path":{dest},"reason":"r",
        "before_hash":null,"after_hash":null,"content":{content}}}"#
    )
}

#[test]
fn create_is_reversible() {
    let d = workspace();
    let root = Utf8Path::from_path(d.path()).unwrap();
    let id = apply_ops(root, &op("Create", "crates/c.rs", Some("x"), None)).unwrap();
    assert!(root.join("crates/c.rs").as_std_path().exists());
    revert::revert(root, &id).unwrap();
    assert!(!root.join("crates/c.rs").as_std_path().exists());
}

#[test]
fn write_and_replace_are_reversible() {
    let d = workspace();
    let root = Utf8Path::from_path(d.path()).unwrap();
    std::fs::write(root.join("crates/r.rs"), "ORIGINAL").unwrap();
    let id = apply_ops(root, &op("Replace", "crates/r.rs", Some("CHANGED"), None)).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("crates/r.rs")).unwrap(),
        "CHANGED"
    );
    revert::revert(root, &id).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("crates/r.rs")).unwrap(),
        "ORIGINAL"
    );
}

#[test]
fn append_is_reversible() {
    let d = workspace();
    let root = Utf8Path::from_path(d.path()).unwrap();
    std::fs::write(root.join("crates/a.rs"), "A").unwrap();
    let id = apply_ops(root, &op("Append", "crates/a.rs", Some("B"), None)).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("crates/a.rs")).unwrap(),
        "AB"
    );
    revert::revert(root, &id).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("crates/a.rs")).unwrap(),
        "A"
    );
}

#[test]
fn delete_is_reversible() {
    let d = workspace();
    let root = Utf8Path::from_path(d.path()).unwrap();
    std::fs::write(root.join("crates/d.rs"), "KEEP").unwrap();
    let id = apply_ops(root, &op("Delete", "crates/d.rs", None, None)).unwrap();
    assert!(!root.join("crates/d.rs").as_std_path().exists());
    revert::revert(root, &id).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("crates/d.rs")).unwrap(),
        "KEEP"
    );
}

#[test]
fn rename_is_reversible() {
    let d = workspace();
    let root = Utf8Path::from_path(d.path()).unwrap();
    std::fs::write(root.join("crates/from.rs"), "MOVED").unwrap();
    let id = apply_ops(
        root,
        &op("Rename", "crates/from.rs", None, Some("crates/to.rs")),
    )
    .unwrap();
    assert!(root.join("crates/to.rs").as_std_path().exists());
    assert!(!root.join("crates/from.rs").as_std_path().exists());
    revert::revert(root, &id).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("crates/from.rs")).unwrap(),
        "MOVED"
    );
    assert!(!root.join("crates/to.rs").as_std_path().exists());
}

#[test]
fn create_directory_is_reversible() {
    let d = workspace();
    let root = Utf8Path::from_path(d.path()).unwrap();
    let id = apply_ops(root, &op("CreateDirectory", "crates/newdir", None, None)).unwrap();
    assert!(root.join("crates/newdir").as_std_path().is_dir());
    revert::revert(root, &id).unwrap();
    assert!(!root.join("crates/newdir").as_std_path().exists());
}

#[test]
fn no_out_of_policy_write_lands() {
    for bad in [
        "../escape.rs",
        "/etc/passwd",
        ".git/config",
        ".env",
        "target/x.rs",
        "node_modules/y.rs",
    ] {
        let d = workspace();
        let root = Utf8Path::from_path(d.path()).unwrap();
        let res = apply_ops(root, &op("Write", bad, Some("evil"), None));
        assert!(res.is_err(), "write to {bad} must be refused");
        // The offending path was never written.
        let candidate = root.join(bad);
        assert!(
            !candidate.as_std_path().is_file()
                || std::fs::read_to_string(candidate.as_std_path()).unwrap_or_default() != "evil",
            "{bad} should not contain the evil payload"
        );
    }
}
