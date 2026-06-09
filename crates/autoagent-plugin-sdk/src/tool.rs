//! Tool + host contracts (M7). Tools NEVER touch the filesystem or run commands
//! directly — all I/O goes through `HostContext`, which the core routes to the
//! PolicyEngine (SPEC-1 FR-24). This is the plugin ABI frozen at 1.0.0.

use crate::schema::ToolSchema;
use serde_json::Value;

pub type ToolResult = Result<Value, String>;

/// The only I/O surface a tool has. The host implementation (in the core) routes
/// every call through the PolicyEngine; there is no bypass.
pub trait HostContext {
    fn write_file(&mut self, path: &str, content: &str) -> Result<(), String>;
    fn read_file(&mut self, path: &str) -> Result<String, String>;
    fn run_command(&mut self, command: &str) -> Result<String, String>;
}

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn schema(&self) -> ToolSchema;
    fn invoke(&self, input: Value, ctx: &mut dyn HostContext) -> ToolResult;
}

/// A host that denies all I/O — for tools that are pure transforms and for tests.
pub struct NullHost;

impl HostContext for NullHost {
    fn write_file(&mut self, _path: &str, _content: &str) -> Result<(), String> {
        Err("no host I/O available".into())
    }
    fn read_file(&mut self, _path: &str) -> Result<String, String> {
        Err("no host I/O available".into())
    }
    fn run_command(&mut self, _command: &str) -> Result<String, String> {
        Err("no host I/O available".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn tool_invokes() {
        let mut ctx = NullHost;
        let out = Echo.invoke(json!({"text":"hi"}), &mut ctx).unwrap();
        assert_eq!(out["echoed"], "hi");
    }
}
