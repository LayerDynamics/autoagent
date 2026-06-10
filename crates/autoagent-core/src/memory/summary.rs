//! Typed project-memory summary returned by the `memory` binding. Previously an
//! ad-hoc `serde_json::json!` in autoagent-bingen; promoted to a real struct so
//! the SDK model schema (SPEC-2 §0 / SDK plan S1) can type it.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MemorySummary {
    pub name: String,
    pub language: String,
    pub package_manager: Option<String>,
    pub source_file_count: usize,
    pub decisions: usize,
}
