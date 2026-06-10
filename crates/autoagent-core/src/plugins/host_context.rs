//! CoreHost (M7) — the bridge that routes every plugin/tool I/O call through the
//! PolicyEngine (SPEC-1 FR-24). There is NO path from a tool to the filesystem
//! or a command that skips this check.

use crate::safety::policy_engine::PolicyEngine;
use crate::validation::command_runner;
use autoagent_plugin_sdk::tool::HostContext;
use camino::Utf8PathBuf;

pub struct CoreHost {
    root: Utf8PathBuf,
    engine: PolicyEngine,
}

impl CoreHost {
    pub fn new(root: Utf8PathBuf, engine: PolicyEngine) -> Self {
        Self { root, engine }
    }
}

impl HostContext for CoreHost {
    fn write_file(&mut self, path: &str, content: &str) -> Result<(), String> {
        let abs = self
            .engine
            .check_write(Utf8PathBuf::from(path))
            .map_err(|e| e.to_string())?;
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent.as_std_path()).map_err(|e| e.to_string())?;
        }
        std::fs::write(abs.as_std_path(), content).map_err(|e| e.to_string())
    }

    fn read_file(&mut self, path: &str) -> Result<String, String> {
        let abs = self
            .engine
            .check_read(Utf8PathBuf::from(path))
            .map_err(|e| e.to_string())?;
        std::fs::read_to_string(abs.as_std_path()).map_err(|e| e.to_string())
    }

    fn run_command(&mut self, command: &str) -> Result<String, String> {
        // Plugins are sandboxed: allow-listed commands only, never auto-approved.
        let result = command_runner::run_one(command, self.root.clone(), &self.engine, false)
            .map_err(|e| e.to_string())?;
        if result.exit_code == Some(0) {
            Ok(result.stdout)
        } else {
            Err(format!(
                "command exited {:?}: {}",
                result.exit_code, result.stderr
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::config_schema::AutoAgentConfig;
    use crate::config::default_config;

    fn host(root: &camino::Utf8Path) -> CoreHost {
        let real = std::fs::canonicalize(root.as_std_path()).unwrap();
        let real = camino::Utf8PathBuf::from_path_buf(real).unwrap();
        let cfg = AutoAgentConfig::from_toml_str(&default_config::default_toml()).unwrap();
        let engine = PolicyEngine::from_config(&cfg, real.clone());
        CoreHost::new(real, engine)
    }

    #[test]
    fn host_rejects_blocked_write() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let mut h = host(root);
        assert!(h.write_file(".git/config", "x").is_err());
        assert!(h.write_file("crates/ok.rs", "x").is_ok());
        assert_eq!(
            std::fs::read_to_string(
                std::fs::canonicalize(root.as_std_path())
                    .unwrap()
                    .join("crates/ok.rs")
            )
            .unwrap(),
            "x"
        );
    }

    #[test]
    fn host_rejects_blocked_command() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let mut h = host(root);
        assert!(h.run_command("sudo rm -rf /").is_err());
    }
}
