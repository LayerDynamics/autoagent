//! Plugin + manifest contracts (M7).

use crate::tool::Tool;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub description: String,
    pub tools: Vec<String>,
}

pub trait Plugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;
    fn tools(&self) -> Vec<Box<dyn Tool>>;
}
