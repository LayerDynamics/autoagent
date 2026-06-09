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
) -> Result<CommandValidationResult> {
    engine.check_command(cmd)?;
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
) -> Result<ValidationReport> {
    let mut commands = Vec::new();
    for c in cmds {
        commands.push(run_one(c, cwd.clone(), engine)?);
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
        let r = run_one("cargo --version", ".".into(), &allow(&["cargo --version"])).unwrap();
        assert_eq!(r.exit_code, Some(0));
        assert!(r.stdout.contains("cargo"));
    }

    #[test]
    fn blocked_command_is_policy_error() {
        let res = run_one("sudo rm -rf /", ".".into(), &allow(&[]));
        match res {
            Err(e) => assert_eq!(e.error_code(), "policy.blocked_command"),
            Ok(_) => panic!("blocked command must not run"),
        }
    }

    #[test]
    fn run_all_reports_pass() {
        let rep = run_all(
            &["cargo --version".to_string()],
            ".".into(),
            &allow(&["cargo --version"]),
        )
        .unwrap();
        assert!(rep.passed);
        assert_eq!(rep.commands.len(), 1);
    }
}
