//! Command runner (M4) — executes policy-approved validation commands and
//! captures structured results (SPEC-1 FR-11, §3.2 validation). A blocked
//! command never runs; the policy error bubbles up before execution.

use crate::error::{AutoAgentError, Result};
use crate::safety::policy_engine::PolicyEngine;
use crate::validation::validation_report::{CommandValidationResult, ValidationReport};
use camino::Utf8PathBuf;
use std::time::Instant;

pub fn run_one(
    cmd: &str,
    cwd: Utf8PathBuf,
    engine: &PolicyEngine,
    approved: bool,
) -> Result<CommandValidationResult> {
    // Allow-listed commands always run; a clean unknown command runs only when
    // the run is approved (so the agent can pursue the tools it needs); blocked /
    // unsafe commands never run.
    engine.authorize_command(cmd, approved)?;
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err(AutoAgentError::Validation("empty command".into()));
    }
    let started = Instant::now();
    let output = std::process::Command::new(parts[0])
        .args(&parts[1..])
        .current_dir(cwd.as_std_path())
        .output()
        .map_err(|e| AutoAgentError::Validation(format!("{cmd}: {e}")))?;
    Ok(CommandValidationResult {
        command: cmd.to_string(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        duration_ms: started.elapsed().as_millis(),
    })
}

pub fn run_all(
    cmds: &[String],
    cwd: Utf8PathBuf,
    engine: &PolicyEngine,
    approved: bool,
) -> Result<ValidationReport> {
    let mut commands = Vec::new();
    for c in cmds {
        commands.push(run_one(c, cwd.clone(), engine, approved)?);
    }
    let passed = commands.iter().all(|r| r.exit_code == Some(0));
    Ok(ValidationReport { passed, commands })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::config_schema::AutoAgentConfig;
    use crate::config::default_config;

    fn allow(cmds: &[&str]) -> PolicyEngine {
        let mut cfg = AutoAgentConfig::from_toml_str(&default_config::default_toml()).unwrap();
        cfg.safety.allowed_commands = cmds.iter().map(|s| s.to_string()).collect();
        PolicyEngine::from_config(&cfg, ".".into())
    }

    #[test]
    fn runs_allowed_command_and_captures_output() {
        let r = run_one(
            "cargo --version",
            ".".into(),
            &allow(&["cargo --version"]),
            false,
        )
        .unwrap();
        assert_eq!(r.exit_code, Some(0));
        assert!(r.stdout.contains("cargo"));
    }

    #[test]
    fn blocked_command_is_policy_error() {
        // Even when approved, a hard-blocked command must never run.
        let res = run_one("sudo rm -rf /", ".".into(), &allow(&[]), true);
        match res {
            Err(e) => assert_eq!(e.error_code(), "policy.blocked_command"),
            Ok(_) => panic!("blocked command must not run"),
        }
    }

    #[test]
    fn unknown_clean_command_runs_only_when_approved() {
        // `cargo --version` is not on the (empty) allow-list, so it is unknown
        // but clean: refused without approval, runs WITH approval.
        assert!(run_one("cargo --version", ".".into(), &allow(&[]), false).is_err());
        let r = run_one("cargo --version", ".".into(), &allow(&[]), true).unwrap();
        assert_eq!(r.exit_code, Some(0));
    }

    #[test]
    fn run_all_reports_pass() {
        let rep = run_all(
            &["cargo --version".to_string()],
            ".".into(),
            &allow(&["cargo --version"]),
            false,
        )
        .unwrap();
        assert!(rep.passed);
        assert_eq!(rep.commands.len(), 1);
    }
}
