//! Config loading tests live here; the load logic is on `AutoAgentConfig`
//! (see `config_schema`).

#[cfg(test)]
mod tests {
    use crate::config::config_schema::AutoAgentConfig;
    use crate::config::default_config;

    #[test]
    fn loads_default_config() {
        let cfg = AutoAgentConfig::from_toml_str(&default_config::default_toml()).unwrap();
        assert_eq!(cfg.agent.max_steps_per_run, 12);
        assert!(!cfg.agent.allow_self_modification);
        assert!(cfg.safety.blocked_write_paths.iter().any(|p| p == ".git/"));
        assert!(cfg
            .safety
            .allowed_commands
            .iter()
            .any(|c| c == "cargo test"));
    }

    #[test]
    fn invalid_toml_is_config_error() {
        let err = AutoAgentConfig::from_toml_str("not valid toml {{{").unwrap_err();
        assert_eq!(err.error_code(), "config");
    }

    #[test]
    fn missing_file_is_config_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let err = AutoAgentConfig::load(root).unwrap_err();
        assert_eq!(err.error_code(), "config");
    }
}
