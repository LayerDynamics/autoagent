//! Project analysis result types (M2).
//!
//! PROPOSED DESIGN (SPEC-1 §13 names the deliverables but no schema): these
//! types are an M2 design decision, owned by this milestone.

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub enum LanguageKind {
    Rust,
    JavaScript,
    TypeScript,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub enum PackageManager {
    Cargo,
    Npm,
    Pnpm,
    Yarn,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DependencySummary {
    pub name: String,
    pub version: String,
    pub dev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectAnalysis {
    // `Utf8PathBuf` has no JsonSchema impl (camino lacks a schemars feature); it
    // serializes as a string, so describe it as one in the schema.
    #[schemars(with = "String")]
    pub root: Utf8PathBuf,
    pub language: LanguageKind,
    pub package_manager: Option<PackageManager>,
    pub dependencies: Vec<DependencySummary>,
    pub file_count: usize,
    pub source_files: usize,
    pub top_dirs: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_serializes() {
        let a = ProjectAnalysis {
            root: "/ws".into(),
            language: LanguageKind::Rust,
            package_manager: Some(PackageManager::Cargo),
            dependencies: vec![DependencySummary {
                name: "serde".into(),
                version: "1".into(),
                dev: false,
            }],
            file_count: 12,
            source_files: 8,
            top_dirs: vec!["crates".into(), "docs".into()],
        };
        let j = serde_json::to_string(&a).unwrap();
        assert!(j.contains("\"language\":\"Rust\""));
        assert!(j.contains("serde"));
    }

    #[test]
    fn project_analysis_has_json_schema() {
        let schema = schemars::schema_for!(ProjectAnalysis);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("source_files"));
    }
}
