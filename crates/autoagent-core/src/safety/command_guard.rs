//! Command guard — the default-deny command authorizer (SPEC-1 §3.7).

use crate::error::{PolicyError, Result};

/// Outcome of an authorized command.
#[derive(Debug, PartialEq, Eq)]
pub enum Approved {
    /// Allowed by an exact policy allow-list entry.
    Policy,
    /// Allowed by an explicit user approval at runtime.
    User,
}

pub struct CommandGuard {
    allowed: Vec<String>,
    blocked: Vec<String>,
}

const BUILTIN_BLOCK: &[&str] = &[
    "sudo",
    "rm -rf /",
    "curl",
    "wget",
    "ssh",
    "scp",
    "chmod 777",
    "chown",
];
const META: &[char] = &[';', '|', '&', '>', '<', '`'];

impl CommandGuard {
    pub fn new(allowed: Vec<String>, blocked: Vec<String>) -> Self {
        Self { allowed, blocked }
    }

    /// Authorize a command string. An UNKNOWN but syntactically-clean command
    /// returns `CommandNotApproved` so the caller can route it to the approval
    /// gate (SPEC-1 §3.7 steps 5–6).
    pub fn check(&self, raw: &str) -> Result<Approved> {
        let canon = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        // 2. blocked substring / fragment (catches "X && sudo Y")
        if self.blocked.iter().any(|b| canon.contains(b.as_str()))
            || BUILTIN_BLOCK.iter().any(|b| canon.contains(b))
        {
            return Err(PolicyError::BlockedCommand(canon).into());
        }
        // 4. exact allow short-circuits the metachar rule
        if self.allowed.iter().any(|a| a == &canon) {
            return Ok(Approved::Policy);
        }
        // 3. metachars are only safe inside an exact allow entry (handled above)
        if canon.contains(META) || canon.contains("$(") {
            return Err(PolicyError::UnsafeShellSyntax(canon).into());
        }
        // 5. unknown but clean → caller routes to approval gate
        Err(PolicyError::CommandNotApproved(canon).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn guard() -> CommandGuard {
        CommandGuard::new(
            vec!["cargo test".into(), "cargo build".into()],
            vec!["sudo".into(), "rm -rf /".into(), "curl".into()],
        )
    }

    fn code(e: &crate::error::AutoAgentError) -> String {
        e.error_code()
            .strip_prefix("policy.")
            .unwrap_or("")
            .to_string()
    }

    #[test]
    fn allows_exact() {
        assert!(matches!(guard().check("cargo test"), Ok(Approved::Policy)));
    }

    #[test]
    fn collapses_whitespace_for_exact_match() {
        assert!(matches!(
            guard().check("  cargo   test  "),
            Ok(Approved::Policy)
        ));
    }

    #[test]
    fn blocks_fragment_in_chain() {
        let e = guard().check("cargo test && sudo rm x").unwrap_err();
        assert_eq!(code(&e), "blocked_command");
    }

    #[test]
    fn rejects_shell_metachars_on_nonexact() {
        let e = guard().check("cargo test > /etc/passwd").unwrap_err();
        assert_eq!(code(&e), "unsafe_shell_syntax");
    }

    #[test]
    fn unknown_clean_command_needs_approval() {
        let e = guard().check("cargo nextest run").unwrap_err();
        assert_eq!(code(&e), "command_not_approved");
    }

    proptest! {
        #[test]
        fn fuzz_blocked_substring_always_caught(pre in "[a-z ]{0,10}") {
            let cmd = format!("{pre}sudo x");
            prop_assert!(guard().check(&cmd).is_err());
        }
    }
}
