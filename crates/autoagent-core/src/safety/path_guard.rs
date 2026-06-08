//! Path guard — the default-deny path authorizer (SPEC-1 §3.7).
//!
//! Precedence (highest first): escape > symlink-escape > block > allow.
//! A path is allowed only if it survives every step.

use crate::error::{PolicyError, Result};
use camino::{Utf8Path, Utf8PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    Read,
    Write,
}

pub struct PathGuard {
    root: Utf8PathBuf,
    allowed_write: Vec<String>,
    blocked_write: Vec<String>,
}

impl PathGuard {
    pub fn new(root: Utf8PathBuf, allowed_write: Vec<String>, blocked_write: Vec<String>) -> Self {
        Self {
            root,
            allowed_write,
            blocked_write,
        }
    }

    /// Authorize `path` for the requested access, returning the normalized
    /// absolute path on success.
    pub fn check(&self, path: Utf8PathBuf, access: Access) -> Result<Utf8PathBuf> {
        // 1. empty / NUL
        if path.as_str().is_empty() || path.as_str().contains('\0') {
            return Err(PolicyError::PathEscape(path.to_string()).into());
        }
        // 2. canonicalize intent lexically (no disk touch, so create-targets work)
        let joined = if path.is_absolute() {
            path.clone()
        } else {
            self.root.join(&path)
        };
        let normalized = lexical_normalize(&joined);
        // 3. escape check
        if !normalized.starts_with(&self.root) {
            return Err(PolicyError::PathEscape(path.to_string()).into());
        }
        let rel = normalized.strip_prefix(&self.root).unwrap_or(&normalized);
        // 4. symlink resolution (best effort) on the longest existing prefix
        if symlink_escapes(&normalized, &self.root) {
            return Err(PolicyError::SymlinkEscape(path.to_string()).into());
        }
        // 5. block list (highest precedence among path-membership rules)
        if self.blocked_write.iter().any(|b| under(rel, b)) || builtin_denied(rel) {
            return Err(PolicyError::BlockedPath(path.to_string()).into());
        }
        // 6. allow (write only); reads only need to be inside the workspace
        if access == Access::Write && !self.allowed_write.iter().any(|a| under(rel, a)) {
            return Err(PolicyError::NotAllowed(path.to_string()).into());
        }
        Ok(normalized)
    }
}

/// Lexically normalize a path, preserving a leading root, without touching disk.
fn lexical_normalize(p: &Utf8Path) -> Utf8PathBuf {
    use camino::Utf8Component::*;
    let mut out: Vec<&str> = Vec::new();
    let mut is_abs = false;
    for c in p.components() {
        match c {
            Prefix(_) => {}
            RootDir => {
                is_abs = true;
                out.clear();
            }
            CurDir => {}
            ParentDir => {
                out.pop();
            }
            Normal(s) => out.push(s),
        }
    }
    let joined = out.join("/");
    if is_abs {
        Utf8PathBuf::from(format!("/{joined}"))
    } else {
        Utf8PathBuf::from(joined)
    }
}

/// True when `rel` is, or is contained within, the policy `prefix`.
fn under(rel: &Utf8Path, prefix: &str) -> bool {
    let pfx = prefix.trim_end_matches('/');
    if pfx.is_empty() {
        return false;
    }
    let s = rel.as_str();
    s == pfx || s.starts_with(&format!("{pfx}/")) || rel.file_name() == Some(pfx)
}

/// Paths denied regardless of config (SPEC-1 §3.7 built-in deny).
fn builtin_denied(rel: &Utf8Path) -> bool {
    let s = rel.as_str();
    s.starts_with(".git/")
        || s == ".git"
        || s.starts_with("node_modules/")
        || s.contains("/node_modules/")
        || rel
            .file_name()
            .map(|n| n.starts_with(".env"))
            .unwrap_or(false)
}

/// Detect a symlink in the existing path prefix whose real target escapes root.
/// Skips the check when root does not exist on disk (e.g. unit tests).
fn symlink_escapes(candidate: &Utf8Path, root: &Utf8Path) -> bool {
    let real_root = match std::fs::canonicalize(root.as_std_path()) {
        Ok(p) => p,
        Err(_) => return false, // root not on disk → nothing to resolve
    };
    let mut cur = candidate.to_path_buf();
    loop {
        if cur.as_std_path().exists() {
            // Existing ancestor above root → nothing inside the workspace to check.
            if !cur.starts_with(root) {
                return false;
            }
            return match std::fs::canonicalize(cur.as_std_path()) {
                Ok(real_cur) => !real_cur.starts_with(&real_root),
                Err(_) => false,
            };
        }
        match cur.parent() {
            Some(p) => cur = p.to_path_buf(),
            None => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn guard() -> PathGuard {
        PathGuard::new(
            Utf8PathBuf::from("/ws"),
            vec!["crates/".into(), "src/".into(), "README.md".into()],
            vec![".git/".into(), "target/".into(), ".env".into()],
        )
    }

    #[test]
    fn rejects_parent_escape() {
        let e = guard()
            .check("../secret".into(), Access::Write)
            .unwrap_err();
        assert_eq!(policy_code(&e), "path_escape");
    }

    #[test]
    fn rejects_absolute_outside() {
        let e = guard()
            .check("/etc/passwd".into(), Access::Write)
            .unwrap_err();
        assert_eq!(policy_code(&e), "path_escape");
    }

    #[test]
    fn block_overrides_allow() {
        // crates/.env is under allowed `crates/` but matches blocked `.env`
        let e = guard()
            .check("crates/.env".into(), Access::Write)
            .unwrap_err();
        assert_eq!(policy_code(&e), "blocked_path");
    }

    #[test]
    fn allows_normal_write() {
        let ok = guard()
            .check("crates/x/src/lib.rs".into(), Access::Write)
            .unwrap();
        assert_eq!(ok, Utf8PathBuf::from("/ws/crates/x/src/lib.rs"));
    }

    #[test]
    fn write_outside_allowlist_rejected() {
        let e = guard()
            .check("docs/readme.md".into(), Access::Write)
            .unwrap_err();
        assert_eq!(policy_code(&e), "not_allowed");
    }

    #[test]
    fn dotdot_resolving_back_inside_is_allowed() {
        // ../ws/crates/x resolves to /ws/crates/x — legitimately inside.
        let ok = guard()
            .check("../ws/crates/x".into(), Access::Write)
            .unwrap();
        assert_eq!(ok, Utf8PathBuf::from("/ws/crates/x"));
    }

    proptest! {
        // The real safety invariant: any path the guard ALLOWS for Write is
        // inside root, under an allowed prefix, and not blocked.
        #[test]
        fn fuzz_allowed_paths_are_inside_and_allowed(s in "[a-z0-9_./]{0,30}") {
            if let Ok(p) = guard().check(Utf8PathBuf::from(s), Access::Write) {
                prop_assert!(p.starts_with("/ws"));
                let rel = p.strip_prefix("/ws").unwrap();
                prop_assert!(["crates", "src"].iter().any(|a| rel.starts_with(a))
                    || rel.as_str() == "README.md");
                prop_assert!(!rel.as_str().contains(".env"));
                prop_assert!(!rel.as_str().starts_with(".git"));
            }
        }
    }

    fn policy_code(e: &crate::error::AutoAgentError) -> String {
        e.error_code()
            .strip_prefix("policy.")
            .unwrap_or("")
            .to_string()
    }
}
