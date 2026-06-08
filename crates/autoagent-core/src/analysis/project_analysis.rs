//! Project analysis result types (M2).
//!
//! PROPOSED DESIGN (SPEC-1 §13 names the deliverables but no schema): these
//! types are an M2 design decision, owned by this milestone.

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LanguageKind {
    Rust,
    JavaScript,
    TypeScript,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PackageManager {
    Cargo,
    Npm,
    Pnpm,
    Yarn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySummary {
    pub name: String,
    pub version: String,
    pub dev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAnalysis {
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
}
