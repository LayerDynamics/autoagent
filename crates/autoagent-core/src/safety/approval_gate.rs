//! Approval gate (SPEC-1 §3.7). The trait lets the CLI inject an interactive
//! prompt while tests and `--yes` inject a non-interactive decision.

use crate::error::{PolicyError, Result};

pub trait ApprovalGate {
    fn confirm_write(&self, target: &str) -> Result<()>;
    fn confirm_command(&self, command: &str) -> Result<()>;
}

/// Non-interactive gate for tests and `--yes`.
pub struct AutoGate {
    yes: bool,
}

impl AutoGate {
    pub fn allow() -> Self {
        Self { yes: true }
    }
    pub fn deny() -> Self {
        Self { yes: false }
    }
}

impl ApprovalGate for AutoGate {
    fn confirm_write(&self, target: &str) -> Result<()> {
        if self.yes {
            Ok(())
        } else {
            Err(PolicyError::WriteNotApproved(target.into()).into())
        }
    }
    fn confirm_command(&self, command: &str) -> Result<()> {
        if self.yes {
            Ok(())
        } else {
            Err(PolicyError::CommandNotApproved(command.into()).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_deny_blocks_write() {
        assert!(AutoGate::deny().confirm_write("crates/x.rs").is_err());
    }

    #[test]
    fn auto_allow_passes() {
        assert!(AutoGate::allow().confirm_write("crates/x.rs").is_ok());
        assert!(AutoGate::allow().confirm_command("cargo test").is_ok());
    }
}
