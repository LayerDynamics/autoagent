//! Safety-parity E2E (FR-6, no-bypass): an apply driven through the binding must
//! produce the *same* mutation artifacts as the real `autoagent` CLI for the
//! same plan, and must be revertible through the binding.
//!
//! No mocked layers: the binding path runs in-process through real core; the CLI
//! path shells out to the actual `autoagent` binary. Client -> binding/CLI ->
//! core -> filesystem.

use std::path::{Path, PathBuf};
use std::process::Command;

const PLAN: &str = r#"{"objective":"contract","summary":"s","files_to_read":[],
"files_to_create":[{"path":"crates/x.rs","purpose":"p"}],"files_to_modify":[],
"operations":[{"kind":"Create","path":"crates/x.rs","destination_path":null,"reason":"r",
  "before_hash":null,"after_hash":null,"content":"// x"}],
"validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#;

/// Build (idempotently) and locate the real `autoagent` CLI binary in the
/// workspace target dir for the current profile.
fn cli_binary() -> PathBuf {
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "autoagent-cli"])
        .status()
        .expect("spawn cargo build for autoagent-cli");
    assert!(status.success(), "failed to build autoagent-cli");

    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let exe = if cfg!(windows) {
        "autoagent.exe"
    } else {
        "autoagent"
    };
    let path = target.join(profile).join(exe);
    assert!(path.exists(), "autoagent binary not found at {path:?}");
    path
}

/// Initialize a workspace and write the shared plan; returns the plan path.
fn seed(root: &Path) -> PathBuf {
    autoagent_bingen::bind::init(root.to_str().unwrap()).unwrap();
    let plan = root.join("p.json");
    std::fs::write(&plan, PLAN).unwrap();
    plan
}

/// The single `.patch` artifact produced under `.agent/patches/`.
fn only_patch(root: &Path) -> String {
    let dir = root.join(".agent/patches");
    let mut patches: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("patches dir exists")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "patch"))
        .collect();
    patches.sort();
    assert_eq!(patches.len(), 1, "expected exactly one patch in {dir:?}");
    std::fs::read_to_string(&patches[0]).unwrap()
}

#[test]
fn binding_apply_matches_cli_apply_and_reverts() {
    let bdir = tempfile::tempdir().unwrap();
    let cdir = tempfile::tempdir().unwrap();
    let bplan = seed(bdir.path());
    let cplan = seed(cdir.path());

    // Binding path: in-process, real core, real policy engine.
    let run_b =
        autoagent_bingen::bind::apply(bdir.path().to_str().unwrap(), bplan.to_str().unwrap(), true)
            .expect("binding apply");

    // CLI path: the real `autoagent` binary on the twin workspace.
    let cli = cli_binary();
    let out = Command::new(&cli)
        .args(["--yes", "apply", cplan.to_str().unwrap()])
        .current_dir(cdir.path())
        .output()
        .expect("run autoagent apply");
    assert!(
        out.status.success(),
        "cli apply failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The mutation artifact is identical (proven byte-equal for this plan).
    assert_eq!(
        only_patch(bdir.path()),
        only_patch(cdir.path()),
        "binding vs CLI patch diverged"
    );
    assert!(
        bdir.path().join("crates/x.rs").exists(),
        "binding apply must create the file"
    );

    // Revert through the binding restores the workspace.
    autoagent_bingen::bind::revert(bdir.path().to_str().unwrap(), &run_b).expect("binding revert");
    assert!(
        !bdir.path().join("crates/x.rs").exists(),
        "revert must restore the workspace"
    );
}
