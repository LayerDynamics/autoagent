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
        // Normalize the root the same way so the prefix comparison is on equal
        // footing across platforms — on Windows the raw root carries a `\`
        // separator and drive prefix (`D:\ws`) that must line up with the
        // lexically normalized candidate.
        let root = lexical_normalize(&self.root);
        // 3. escape check
        if !normalized.starts_with(&root) {
            return Err(PolicyError::PathEscape(path.to_string()).into());
        }
        let rel = normalized.strip_prefix(&root).unwrap_or(&normalized);
        // 4. symlink resolution (best effort) on the longest existing prefix.
        // Use the normalized root so the candidate/root prefix forms match on
        // Windows (both de-verbatimed, forward-slash) — `std::fs::canonicalize`
        // accepts that form and re-adds the verbatim prefix consistently.
        if symlink_escapes(&normalized, &root) {
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

/// Strip a Windows verbatim prefix: `\\?\C:\ws` -> `C:\ws`, `\\?\UNC\srv\sh` ->
/// `\\srv\sh`. Verbatim paths treat `/` as a literal character rather than a
/// separator, which breaks lexical path reasoning; the simplified form parses
/// normally. No-op on Unix and on already-plain paths (string never matches).
///
/// Caveat: this strips unconditionally, whereas the `\\?\` prefix is what lets a
/// path exceed the legacy 260-char `MAX_PATH` limit. A workspace root deep
/// enough that its de-verbatimed run/snapshot paths cross 260 chars, on a system
/// without long-path support, could see IO fail where the verbatim form would
/// have worked. Realistic project roots are far under that, so the trade for
/// `/`-as-separator semantics everywhere is worth it; the follow-up, if it ever
/// bites, is to gate the strip on the resulting length (as the `dunce` crate
/// does) rather than always stripping.
pub(crate) fn simplify_verbatim(p: &Utf8Path) -> Utf8PathBuf {
    let s = p.as_str();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        Utf8PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        Utf8PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

/// Canonicalize a workspace `root` to an absolute, **de-verbatimed** path,
/// falling back to the input when it is not on disk. This is the single source
/// of truth for resolving a run's real root: `std::fs::canonicalize` returns a
/// `\\?\C:\…` verbatim path on Windows, and every downstream `real_root.join(rel)`
/// (file writes, snapshots, run dirs, revert) needs `/`-as-separator semantics —
/// which verbatim paths disable. Stripping the prefix here keeps the whole
/// runtime on plain paths.
pub(crate) fn canonical_root(root: &Utf8Path) -> Utf8PathBuf {
    canonicalize_existing(root).unwrap_or_else(|| simplify_verbatim(root))
}

/// Canonicalize an existing path to its absolute, **de-verbatimed** form
/// (resolves `..`/symlinks via the OS). Returns `None` when the path is not on
/// disk or not valid UTF-8. Use this — never bare `std::fs::canonicalize` —
/// wherever a resolved child path is compared against a [`canonical_root`]:
/// `std::fs::canonicalize` yields a `\\?\` verbatim path on Windows, and a
/// `VerbatimDisk` prefix never `starts_with` a plain `Disk` prefix, so a raw
/// result silently fails containment against the de-verbatimed root.
pub(crate) fn canonicalize_existing(p: &Utf8Path) -> Option<Utf8PathBuf> {
    let real = std::fs::canonicalize(p.as_std_path()).ok()?;
    let utf8 = Utf8PathBuf::from_path_buf(real).ok()?;
    Some(simplify_verbatim(&utf8))
}

/// Lexically normalize a path, preserving a leading root, without touching disk.
///
/// The Windows drive/UNC prefix (`Prefix`, e.g. `C:` or `\\?\C:`) is preserved:
/// dropping it turned an absolute Windows path into a rootless `/...` path that
/// no longer shared a prefix with the (drive-qualified) workspace root, which
/// tripped a false `path_escape` on every relative plan path. On Unix there is
/// no `Prefix` component, so this path is inert there.
fn lexical_normalize(p: &Utf8Path) -> Utf8PathBuf {
    use camino::Utf8Component::*;
    // De-verbatim first: `std::fs::canonicalize` hands back `\\?\C:\…` on
    // Windows, and a verbatim prefix disables `/`-as-separator. Joining a
    // relative plan path (`crates/x.rs`) to such a root left the tail as one
    // un-split component, so the lexical containment check tripped a false
    // `path_escape`. Stripping it makes `/` a real separator again.
    let simplified = simplify_verbatim(p);
    let mut prefix = String::new();
    let mut out: Vec<&str> = Vec::new();
    let mut is_abs = false;
    for c in simplified.components() {
        match c {
            Prefix(pre) => prefix = pre.as_str().to_string(),
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
    let body = if is_abs { format!("/{joined}") } else { joined };
    if prefix.is_empty() {
        Utf8PathBuf::from(body)
    } else {
        // e.g. `C:` + `/ws/crates` -> `C:/ws/crates` (camino parses the drive).
        Utf8PathBuf::from(format!("{prefix}{body}"))
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

    #[cfg(windows)]
    #[test]
    fn windows_drive_root_relative_path_is_inside() {
        // Regression (CI: rust-tests · windows): an absolute Windows root
        // (`D:\ws`) joined with a forward-slash relative plan path
        // (`crates/x.rs`) must stay inside the workspace. Before preserving the
        // drive prefix in `lexical_normalize`, this tripped a false `path_escape`.
        let guard = PathGuard::new(Utf8PathBuf::from(r"D:\ws"), vec!["crates/".into()], vec![]);
        let ok = guard
            .check(Utf8PathBuf::from("crates/x.rs"), Access::Write)
            .expect("relative path under a Windows drive root must be allowed");
        assert!(ok.starts_with(r"D:\ws"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_verbatim_root_relative_path_is_inside() {
        // Regression (CI: rust-tests / pyo3 / sdk-python / sdk-node · windows):
        // `std::fs::canonicalize` hands back a verbatim root (`\\?\C:\ws`), where
        // `/` is a literal char, not a separator. Joining `crates/x.rs` then left
        // the tail un-split and tripped a false `path_escape` on EVERY relative
        // apply. `simplify_verbatim` strips the prefix so containment holds.
        let guard = PathGuard::new(
            Utf8PathBuf::from(r"\\?\C:\ws"),
            vec!["crates/".into()],
            vec![],
        );
        let ok = guard
            .check(Utf8PathBuf::from("crates/x.rs"), Access::Write)
            .expect("relative path under a verbatim Windows root must be allowed");
        assert!(ok.as_str().ends_with("crates/x.rs"));
    }

    #[test]
    fn simplify_verbatim_strips_windows_prefix() {
        // Pure string logic, so it runs on every platform: the de-verbatim step
        // that makes the Windows containment check work.
        assert_eq!(
            simplify_verbatim(Utf8Path::new(r"\\?\C:\ws")).as_str(),
            r"C:\ws"
        );
        assert_eq!(
            simplify_verbatim(Utf8Path::new(r"\\?\UNC\srv\share")).as_str(),
            r"\\srv\share"
        );
        // Plain paths pass through untouched.
        assert_eq!(
            simplify_verbatim(Utf8Path::new("/ws/crates")).as_str(),
            "/ws/crates"
        );
        assert_eq!(
            simplify_verbatim(Utf8Path::new("crates/x.rs")).as_str(),
            "crates/x.rs"
        );
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
