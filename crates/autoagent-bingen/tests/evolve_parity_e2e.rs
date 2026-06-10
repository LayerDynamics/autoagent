//! Safety-parity E2E for the self-authoring `evolve --apply` path (FR-6, the
//! most dangerous operation): an evolve driven through the binding must produce
//! the same mutation artifact as the real `autoagent` CLI for the same plan.
//!
//! evolve --apply is gated by `allow_self_modification = true` and branches the
//! workspace first, so each twin is a clean git repo with self-mod enabled.
//! No mocked layers: binding path runs in-process core; CLI path shells out.

use std::path::{Path, PathBuf};
use std::process::Command;

const PLAN: &str = r#"{"objective":"contract","summary":"s","files_to_read":[],
"files_to_create":[{"path":"crates/x.rs","purpose":"p"}],"files_to_modify":[],
"operations":[{"kind":"Create","path":"crates/x.rs","destination_path":null,"reason":"r",
  "before_hash":null,"after_hash":null,"content":"// x"}],
"validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#;

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

fn git(root: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git")
        .status
        .success();
    assert!(ok, "git {args:?} failed");
}

/// Init a workspace with self-modification enabled, write the plan, and make it
/// a clean git repo (evolve branches before applying).
fn seed(root: &Path) -> PathBuf {
    autoagent_bingen::bind::init(root.to_str().unwrap()).unwrap();
    // Enable self-modification (default is false → evolve --apply is refused).
    let toml = root.join("Autoagent.toml");
    let cfg = std::fs::read_to_string(&toml).unwrap();
    std::fs::write(
        &toml,
        cfg.replace(
            "allow_self_modification = false",
            "allow_self_modification = true",
        ),
    )
    .unwrap();
    let plan = root.join("p.json");
    std::fs::write(&plan, PLAN).unwrap();

    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "seed"]);
    plan
}

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
fn binding_evolve_apply_matches_cli_evolve_apply() {
    let bdir = tempfile::tempdir().unwrap();
    let cdir = tempfile::tempdir().unwrap();
    let bplan = seed(bdir.path());
    let cplan = seed(cdir.path());

    // Binding path: in-process core, self-mod gate, real branch + apply.
    let outcome = autoagent_bingen::bind::evolve_sync(
        bdir.path().to_str().unwrap(),
        "contract",
        Some(bplan.to_str().unwrap()),
        true,
    )
    .expect("binding evolve --apply");
    assert!(outcome.contains("\"applied\":true"), "evolve must apply");

    // CLI path: the real `autoagent evolve --from <plan> --apply`.
    let cli = cli_binary();
    let out = Command::new(&cli)
        .args([
            "--yes",
            "evolve",
            "contract",
            "--from",
            cplan.to_str().unwrap(),
            "--apply",
        ])
        .current_dir(cdir.path())
        .output()
        .expect("run autoagent evolve");
    assert!(
        out.status.success(),
        "cli evolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The self-authoring mutation artifact is identical.
    assert_eq!(
        only_patch(bdir.path()),
        only_patch(cdir.path()),
        "binding vs CLI evolve patch diverged"
    );
}

#[test]
fn binding_evolve_apply_refused_without_self_mod() {
    // Without allow_self_modification, evolve --apply must refuse (policy).
    let dir = tempfile::tempdir().unwrap();
    autoagent_bingen::bind::init(dir.path().to_str().unwrap()).unwrap();
    let plan = dir.path().join("p.json");
    std::fs::write(&plan, PLAN).unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "seed"]);

    let err = autoagent_bingen::bind::evolve_sync(
        dir.path().to_str().unwrap(),
        "contract",
        Some(plan.to_str().unwrap()),
        true,
    )
    .unwrap_err();
    assert!(err.code.starts_with("policy"), "got {}", err.code);
}
