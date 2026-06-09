//! Branch-before-evolve (M6, SPEC-1 §3.7 self-modification). Self-authoring is
//! isolated onto `autoagent/evolve/<run-id>` so it never touches the checked-out
//! branch. Refuses to start from a dirty tree.

use crate::error::{AutoAgentError, Result};
use crate::git::git_client;
use camino::Utf8Path;

pub fn evolve_branch_name(run_id: &str) -> String {
    format!("autoagent/evolve/{run_id}")
}

/// Create and check out the isolated evolve branch; returns its name.
pub fn branch_before_evolve(root: &Utf8Path, run_id: &str) -> Result<String> {
    if !git_client::is_clean(root)? {
        return Err(AutoAgentError::Workspace(
            "working tree is dirty; commit or stash before evolve --apply".into(),
        ));
    }
    let branch = evolve_branch_name(run_id);
    let out = std::process::Command::new("git")
        .args(["checkout", "-b", &branch])
        .current_dir(root.as_std_path())
        .output()
        .map_err(|e| AutoAgentError::Workspace(format!("git checkout -b: {e}")))?;
    if out.status.success() {
        Ok(branch)
    } else {
        Err(AutoAgentError::Workspace(format!(
            "failed to create branch {branch}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let path = d.path();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(path)
            .output()
            .unwrap();
        std::fs::write(path.join("seed"), "x").unwrap();
        for args in [
            ["add", "-A"].as_slice(),
            ["commit", "-m", "seed"].as_slice(),
        ] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(path)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap();
        }
        d
    }

    #[test]
    fn creates_and_checks_out_evolve_branch() {
        let d = seeded_repo();
        let root = camino::Utf8Path::from_path(d.path()).unwrap();
        let branch = branch_before_evolve(root, "20260608T000000Z-x").unwrap();
        assert_eq!(branch, "autoagent/evolve/20260608T000000Z-x");
        assert_eq!(git_client::current_branch(root).unwrap(), branch);
    }

    #[test]
    fn refuses_dirty_tree() {
        let d = seeded_repo();
        let root = camino::Utf8Path::from_path(d.path()).unwrap();
        std::fs::write(root.join("dirty.txt"), "y").unwrap();
        assert!(branch_before_evolve(root, "r").is_err());
    }
}
