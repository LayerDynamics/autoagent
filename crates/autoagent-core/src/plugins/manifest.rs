//! Plugin manifest loading (M7) — reads `plugin.toml` from a plugin directory
//! under `.agent/tools/`.

use crate::error::{AutoAgentError, Result};
use autoagent_plugin_sdk::PluginManifest;
use camino::Utf8Path;

pub fn load_manifest(dir: &Utf8Path) -> Result<PluginManifest> {
    let path = dir.join("plugin.toml");
    let text = std::fs::read_to_string(path.as_std_path())
        .map_err(|e| AutoAgentError::Plugin(format!("cannot read {path}: {e}")))?;
    toml::from_str(&text).map_err(|e| AutoAgentError::Plugin(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("plugin.toml"),
            "name=\"demo\"\nversion=\"0.1.0\"\napi_version=1\ndescription=\"d\"\ntools=[\"echo\"]",
        )
        .unwrap();
        let m = load_manifest(root).unwrap();
        assert_eq!(m.name, "demo");
        assert_eq!(m.api_version, 1);
        assert_eq!(m.tools, vec!["echo".to_string()]);
    }

    #[test]
    fn missing_manifest_is_plugin_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        assert_eq!(load_manifest(root).unwrap_err().error_code(), "plugin");
    }
}
