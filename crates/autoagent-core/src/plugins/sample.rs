//! Built-in sample plugin (M7) — a trivial first-party `echo` tool used by the
//! `tools` command, docs, and tests to demonstrate the plugin pipeline.

use autoagent_plugin_sdk::{HostContext, Plugin, PluginManifest, Tool, ToolResult, ToolSchema};
use serde_json::{json, Value};

pub struct EchoTool;

impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn schema(&self) -> ToolSchema {
        ToolSchema::object(&[("text", "string")])
    }
    fn invoke(&self, input: Value, _ctx: &mut dyn HostContext) -> ToolResult {
        Ok(json!({ "echoed": input["text"] }))
    }
}

pub struct SamplePlugin;

impl Plugin for SamplePlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            name: "sample".into(),
            version: "0.1.0".into(),
            api_version: 1,
            description: "built-in sample tools".into(),
            tools: vec!["echo".into()],
        }
    }
    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![Box::new(EchoTool)]
    }
}
