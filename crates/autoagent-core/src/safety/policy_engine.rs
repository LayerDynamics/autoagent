//! Policy engine — composes the path and command guards from config and is the
//! single chokepoint every privileged operation passes through (SPEC-1 §3.7).

use crate::config::config_schema::AutoAgentConfig;
use crate::error::Result;
use crate::safety::command_guard::{Approved, CommandGuard};
use crate::safety::path_guard::{Access, PathGuard};
use camino::Utf8PathBuf;

pub struct PolicyEngine {
    paths: PathGuard,
    commands: CommandGuard,
}

impl PolicyEngine {
    pub fn from_config(cfg: &AutoAgentConfig, root: Utf8PathBuf) -> Self {
        Self {
            paths: PathGuard::new(
                root,
                cfg.safety.allowed_write_paths.clone(),
                cfg.safety.blocked_write_paths.clone(),
            ),
            commands: CommandGuard::new(
                cfg.safety.allowed_commands.clone(),
                cfg.safety.blocked_commands.clone(),
            ),
        }
    }

    pub fn check_write(&self, path: Utf8PathBuf) -> Result<Utf8PathBuf> {
        self.paths.check(path, Access::Write)
    }

    pub fn check_read(&self, path: Utf8PathBuf) -> Result<Utf8PathBuf> {
        self.paths.check(path, Access::Read)
    }

    pub fn check_command(&self, command: &str) -> Result<Approved> {
        self.commands.check(command)
    }

    /// Authorize a command for EXECUTION, given a runtime approval decision.
    /// Allow-listed commands always pass; an unknown-but-clean command passes
    /// only when `approved` (the user opted in via `--yes`/interactive approval/
    /// autonomous mode) — this is how the agent pursues the tools it needs
    /// (installers, linters, scripts, …). Hard-blocked commands (sudo, curl,
    /// wget, ssh, rm -rf, chmod 777, …) and shell-metacharacter/`$()` syntax are
    /// the absolute floor and are NEVER authorized, approval or not.
    pub fn authorize_command(&self, command: &str, approved: bool) -> Result<()> {
        match self.commands.check(command) {
            Ok(_) => Ok(()),
            Err(e) if e.error_code() == "policy.command_not_approved" && approved => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::config_schema::AutoAgentConfig;
    use crate::config::default_config;

    fn engine() -> PolicyEngine {
        let cfg = AutoAgentConfig::from_toml_str(&default_config::default_toml()).unwrap();
        PolicyEngine::from_config(&cfg, "/ws".into())
    }

    #[test]
    fn engine_built_from_config_blocks_git() {
        assert!(engine().check_write(".git/config".into()).is_err());
    }

    #[test]
    fn engine_allows_configured_command() {
        assert!(engine().check_command("cargo test").is_ok());
    }

    #[test]
    fn authorize_command_gates_unknown_on_approval_but_blocks_floor() {
        let e = engine();
        // Allow-listed: always authorized.
        assert!(e.authorize_command("cargo test", false).is_ok());
        // Unknown but clean (e.g. a tool to install/use): needs approval.
        assert!(e.authorize_command("npm install", false).is_err());
        assert!(e.authorize_command("npm install", true).is_ok());
        // Hard floor: blocked even WITH approval.
        assert!(e.authorize_command("sudo rm -rf /", true).is_err());
        assert!(e.authorize_command("curl http://x", true).is_err());
        assert!(e.authorize_command("echo a && sudo b", true).is_err());
    }

    #[test]
    fn engine_allows_configured_write_path() {
        assert!(engine().check_write("crates/x/lib.rs".into()).is_ok());
    }
}
