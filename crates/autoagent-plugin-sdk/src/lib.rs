//! autoagent-plugin-sdk — the stable plugin ABI (M7, frozen at 1.0.0).
//!
//! Defines the `Plugin`/`Tool`/`HostContext` contracts that both native and
//! WASM plugins implement. All tool I/O flows through `HostContext`, which the
//! core routes through the PolicyEngine — no plugin can bypass the safety layer
//! (SPEC-1 FR-24).

pub mod plugin;
pub mod schema;
pub mod tool;

pub use plugin::{Plugin, PluginManifest};
pub use schema::ToolSchema;
pub use tool::{HostContext, NullHost, Tool, ToolResult};

/// The plugin API version this SDK implements. The registry refuses plugins
/// declaring a higher `api_version`.
pub const SUPPORTED_API_VERSION: u32 = 1;
