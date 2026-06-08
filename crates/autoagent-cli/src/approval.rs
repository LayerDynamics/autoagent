//! Interactive approval gate backed by `dialoguer` (SPEC-1 §3.7). The CLI owns
//! all human interaction; the core consumes the `ApprovalGate` trait.

use autoagent_core::error::{PolicyError, Result};
use autoagent_core::safety::approval_gate::ApprovalGate;
use dialoguer::Confirm;

pub struct DialoguerGate;

impl ApprovalGate for DialoguerGate {
    fn confirm_write(&self, target: &str) -> Result<()> {
        let ok = Confirm::new()
            .with_prompt(format!("Apply write to {target}?"))
            .default(false)
            .interact()
            .unwrap_or(false);
        if ok {
            Ok(())
        } else {
            Err(PolicyError::WriteNotApproved(target.into()).into())
        }
    }

    fn confirm_command(&self, command: &str) -> Result<()> {
        let ok = Confirm::new()
            .with_prompt(format!("Run command: {command}?"))
            .default(false)
            .interact()
            .unwrap_or(false);
        if ok {
            Ok(())
        } else {
            Err(PolicyError::CommandNotApproved(command.into()).into())
        }
    }
}
