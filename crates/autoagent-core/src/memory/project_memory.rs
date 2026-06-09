//! Rebuild project memory from a fresh analysis (M5) and helpers that surface
//! memory to the planner.

use crate::analysis::project_analysis::{LanguageKind, ProjectAnalysis};
use crate::analysis::project_analyzer;
use crate::config::config_schema::AutoAgentConfig;
use crate::error::Result;
use crate::memory::memory_store::MemoryStore;
use crate::memory::schema::ProjectMemory;
use camino::Utf8Path;

/// Analyze the project and persist a fresh `ProjectMemory`.
pub fn rebuild_project_memory(
    root: &Utf8Path,
    config: &AutoAgentConfig,
    store: &MemoryStore,
) -> Result<ProjectMemory> {
    let analysis = project_analyzer::analyze(root, config)?;
    let pm = ProjectMemory {
        name: project_name(root, &analysis, config),
        language: format!("{:?}", analysis.language),
        package_manager: analysis.package_manager.as_ref().map(|p| format!("{p:?}")),
        last_analyzed: Some(chrono::Utc::now().to_rfc3339()),
        source_file_count: analysis.source_files,
    };
    store.save_project(&pm)?;
    Ok(pm)
}

/// Short summaries of the most recent decisions, for planner context.
pub fn recent_decision_summaries(store: &MemoryStore, n: usize) -> Vec<String> {
    let mut decisions = store.load_decisions().unwrap_or_default();
    decisions.reverse();
    decisions
        .into_iter()
        .take(n)
        .map(|d| format!("{}: {}", d.date, d.decision))
        .collect()
}

fn project_name(root: &Utf8Path, analysis: &ProjectAnalysis, config: &AutoAgentConfig) -> String {
    match analysis.language {
        LanguageKind::Rust | LanguageKind::Mixed => read_cargo_name(root),
        LanguageKind::JavaScript | LanguageKind::TypeScript => read_pkg_name(root),
        LanguageKind::Unknown => None,
    }
    .unwrap_or_else(|| config.project.name.clone())
}

fn read_cargo_name(root: &Utf8Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join("Cargo.toml").as_std_path()).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    value
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(String::from)
}

fn read_pkg_name(root: &Utf8Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join("package.json").as_std_path()).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("name").and_then(|n| n.as_str()).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_populates_project_memory_from_analysis() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"demo\"\nversion=\"0.1.0\"",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "fn a(){}").unwrap();
        let cfg =
            AutoAgentConfig::from_toml_str(&crate::config::default_config::default_toml()).unwrap();
        let store = MemoryStore::new(root.join(".agent/memory"));
        let pm = rebuild_project_memory(root, &cfg, &store).unwrap();
        assert_eq!(pm.name, "demo");
        assert_eq!(pm.language, "Rust");
        assert!(pm.source_file_count >= 1);
        // persisted and reloadable
        assert_eq!(store.load_project().unwrap().name, "demo");
    }
}
