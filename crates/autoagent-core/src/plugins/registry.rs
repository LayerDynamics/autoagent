//! Tool registry (M7) — registers plugins (api-version + dup-name checks) and
//! invokes tools (schema-validated) through a `HostContext`.

use crate::error::{AutoAgentError, Result};
use autoagent_plugin_sdk::{HostContext, Plugin, Tool, SUPPORTED_API_VERSION};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_plugin(&mut self, plugin: Box<dyn Plugin>) -> Result<()> {
        let manifest = plugin.manifest();
        if manifest.api_version > SUPPORTED_API_VERSION {
            return Err(AutoAgentError::Plugin(format!(
                "plugin '{}' requires api_version {} > supported {}",
                manifest.name, manifest.api_version, SUPPORTED_API_VERSION
            )));
        }
        for tool in plugin.tools() {
            let name = tool.name().to_string();
            if self.tools.contains_key(&name) {
                return Err(AutoAgentError::Plugin(format!(
                    "duplicate tool name '{name}'"
                )));
            }
            self.tools.insert(name, tool);
        }
        Ok(())
    }

    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn tool_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn invoke(&self, name: &str, input: Value, host: &mut dyn HostContext) -> Result<Value> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| AutoAgentError::Plugin(format!("unknown tool '{name}'")))?;
        tool.schema()
            .validate(&input)
            .map_err(AutoAgentError::Plugin)?;
        tool.invoke(input, host).map_err(AutoAgentError::Plugin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoagent_plugin_sdk::{NullHost, PluginManifest, ToolResult, ToolSchema};
    use serde_json::json;

    struct Echo;
    impl Tool for Echo {
        fn name(&self) -> &str {
            "echo"
        }
        fn schema(&self) -> ToolSchema {
            ToolSchema::object(&[("text", "string")])
        }
        fn invoke(&self, input: Value, _ctx: &mut dyn HostContext) -> ToolResult {
            Ok(json!({"echoed": input["text"]}))
        }
    }

    struct SamplePlugin;
    impl Plugin for SamplePlugin {
        fn manifest(&self) -> PluginManifest {
            PluginManifest {
                name: "sample".into(),
                version: "0.1.0".into(),
                api_version: 1,
                description: "echo".into(),
                tools: vec!["echo".into()],
            }
        }
        fn tools(&self) -> Vec<Box<dyn Tool>> {
            vec![Box::new(Echo)]
        }
    }

    struct FutureApiPlugin;
    impl Plugin for FutureApiPlugin {
        fn manifest(&self) -> PluginManifest {
            PluginManifest {
                name: "future".into(),
                version: "0.1.0".into(),
                api_version: 2,
                description: "x".into(),
                tools: vec![],
            }
        }
        fn tools(&self) -> Vec<Box<dyn Tool>> {
            vec![]
        }
    }

    #[test]
    fn registers_and_invokes_native_tool() {
        let mut reg = ToolRegistry::new();
        reg.register_plugin(Box::new(SamplePlugin)).unwrap();
        assert!(reg.has_tool("echo"));
        let mut host = NullHost;
        let out = reg.invoke("echo", json!({"text":"hi"}), &mut host).unwrap();
        assert_eq!(out["echoed"], "hi");
    }

    #[test]
    fn rejects_duplicate_tool_name() {
        let mut reg = ToolRegistry::new();
        reg.register_plugin(Box::new(SamplePlugin)).unwrap();
        assert!(reg.register_plugin(Box::new(SamplePlugin)).is_err());
    }

    #[test]
    fn rejects_incompatible_api_version() {
        let mut reg = ToolRegistry::new();
        let err = reg.register_plugin(Box::new(FutureApiPlugin)).unwrap_err();
        assert_eq!(err.error_code(), "plugin");
    }

    #[test]
    fn invoke_validates_input_schema() {
        let mut reg = ToolRegistry::new();
        reg.register_plugin(Box::new(SamplePlugin)).unwrap();
        let mut host = NullHost;
        assert!(reg.invoke("echo", json!({}), &mut host).is_err());
    }
}
