pub mod host_context;
pub mod manifest;
pub mod registry;
pub mod sample;
pub mod wasm_host;

/// Build a registry with the built-in first-party plugins registered.
pub fn with_builtins() -> crate::error::Result<registry::ToolRegistry> {
    let mut reg = registry::ToolRegistry::new();
    reg.register_plugin(Box::new(sample::SamplePlugin))?;
    Ok(reg)
}

/// Discover WASM plugin manifests under `.agent/tools/*/plugin.toml`.
pub fn discover_wasm_plugins(root: &camino::Utf8Path) -> Vec<autoagent_plugin_sdk::PluginManifest> {
    let tools_dir = root.join(".agent/tools");
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(tools_dir.as_std_path()) {
        for entry in entries.flatten() {
            if let Ok(dir) = camino::Utf8PathBuf::from_path_buf(entry.path()) {
                if dir.join("plugin.toml").as_std_path().exists() {
                    if let Ok(m) = manifest::load_manifest(&dir) {
                        found.push(m);
                    }
                }
            }
        }
    }
    found
}
