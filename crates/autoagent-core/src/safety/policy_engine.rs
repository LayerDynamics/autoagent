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
    fn engine_allows_configured_write_path() {
        assert!(engine().check_write("crates/x/lib.rs".into()).is_ok());
    }
}
