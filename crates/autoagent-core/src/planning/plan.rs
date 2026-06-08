//! The structured plan — the contract the engine applies (SPEC-1 §3.3 / §8.4).

use crate::editing::file_operation::FileOperation;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub objective: String,
    pub summary: String,
    pub files_to_read: Vec<Utf8PathBuf>,
    pub files_to_create: Vec<PlannedFile>,
    pub files_to_modify: Vec<PlannedFile>,
    pub operations: Vec<FileOperation>,
    pub validation_commands: Vec<String>,
    pub risks: Vec<String>,
    pub rollback_strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedFile {
    pub path: Utf8PathBuf,
    pub purpose: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_roundtrips_json() {
        let json = r#"{"objective":"o","summary":"s","files_to_read":[],
          "files_to_create":[],"files_to_modify":[],
          "operations":[{"kind":"Create","path":"a.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"x"}],
          "validation_commands":["cargo build"],"risks":[],"rollback_strategy":"snapshot"}"#;
        let p: Plan = serde_json::from_str(json).unwrap();
        assert_eq!(p.operations.len(), 1);
        assert_eq!(p.rollback_strategy, "snapshot");
    }
}
