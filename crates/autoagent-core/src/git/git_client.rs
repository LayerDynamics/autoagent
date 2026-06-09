//! Read-only git client (M6) — branch/status/diff via the `git` CLI (the
//! SPEC-1 allowed git commands). Never pushes (SPEC-1 FR-27).

use crate::error::{AutoAgentError, Result};
use camino::Utf8Path;

fn git(root: &Utf8Path, args: &[&str]) -> Result<std::process::Output> {
    std::process::Command::new("git")
        .args(args)
        .current_dir(root.as_std_path())
        .output()
        .map_err(|e| AutoAgentError::Workspace(format!("git {}: {e}", args.join(" "))))
}

/// The current branch name (works on an unborn branch via symbolic-ref).
pub fn current_branch(root: &Utf8Path) -> Result<String> {
    let out = git(root, &["symbolic-ref", "--short", "HEAD"])?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    let out = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(AutoAgentError::Workspace(
            "cannot determine git branch".into(),
        ))
    }
}

/// Porcelain status; empty string means a clean working tree.
pub fn status(root: &Utf8Path) -> Result<String> {
    let out = git(root, &["status", "--porcelain"])?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// True when the working tree has no uncommitted changes.
pub fn is_clean(root: &Utf8Path) -> Result<bool> {
    Ok(status(root)?.trim().is_empty())
}

/// Unified diff of the working tree.
pub fn diff(root: &Utf8Path) -> Result<String> {
    let out = git(root, &["diff"])?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(d.path())
            .output()
            .unwrap();
        d
    }

    #[test]
    fn reports_current_branch() {
        let d = init_repo();
        let root = camino::Utf8Path::from_path(d.path()).unwrap();
        let branch = current_branch(root).unwrap();
        assert!(branch == "main" || branch == "master", "got {branch}");
    }

    #[test]
    fn clean_repo_is_clean_then_dirty() {
        let d = init_repo();
        let root = camino::Utf8Path::from_path(d.path()).unwrap();
        assert!(is_clean(root).unwrap());
        std::fs::write(root.join("new.txt"), "x").unwrap();
        assert!(!is_clean(root).unwrap());
    }
}
