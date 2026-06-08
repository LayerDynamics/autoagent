//! `init` — scaffold `Autoagent.toml` and the `.agent/` workspace
//! (SPEC-1 FR-2). Existing `Autoagent.toml` is preserved, never overwritten.

use crate::config::default_config;
use crate::error::Result;
use camino::Utf8Path;

const AGENT_DIRS: &[&str] = &[
    "memory", "plans", "runs", "patches", "logs", "reports", "tools",
];

/// Returns true if a new `Autoagent.toml` was written (false if one existed).
pub fn init_workspace(root: &Utf8Path) -> Result<bool> {
    let config_path = root.join("Autoagent.toml");
    let wrote_config = if config_path.as_std_path().exists() {
        false
    } else {
        std::fs::write(config_path.as_std_path(), default_config::default_toml())?;
        true
    };
    let agent = root.join(".agent");
    for d in AGENT_DIRS {
        std::fs::create_dir_all(agent.join(d).as_std_path())?;
    }
    Ok(wrote_config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_writes_config_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let wrote = init_workspace(root).unwrap();
        assert!(wrote);
        assert!(root.join("Autoagent.toml").as_std_path().exists());
        assert!(root.join(".agent/runs").as_std_path().exists());
        assert!(root.join(".agent/memory").as_std_path().exists());
    }

    #[test]
    fn init_preserves_existing_config() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("Autoagent.toml"), "# mine\n").unwrap();
        let wrote = init_workspace(root).unwrap();
        assert!(!wrote);
        assert_eq!(
            std::fs::read_to_string(root.join("Autoagent.toml")).unwrap(),
            "# mine\n"
        );
    }
}
