//! Markdown project-analysis report writer (M2, SPEC-1 FR-18).

use crate::analysis::project_analysis::{LanguageKind, PackageManager, ProjectAnalysis};
use crate::error::Result;
use camino::{Utf8Path, Utf8PathBuf};
use std::fmt::Write as _;

/// Render a `ProjectAnalysis` as Markdown.
pub fn render(a: &ProjectAnalysis) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Project Analysis\n");
    let _ = writeln!(s, "| Field | Value |");
    let _ = writeln!(s, "| --- | --- |");
    let _ = writeln!(s, "| Root | `{}` |", a.root);
    let _ = writeln!(s, "| Language | {} |", language_str(&a.language));
    let _ = writeln!(
        s,
        "| Package manager | {} |",
        a.package_manager.as_ref().map(pm_str).unwrap_or("none")
    );
    let _ = writeln!(s, "| Files (in workspace) | {} |", a.file_count);
    let _ = writeln!(s, "| Source files | {} |", a.source_files);
    let _ = writeln!(s, "| Dependencies | {} |\n", a.dependencies.len());

    let _ = writeln!(s, "## Dependencies\n");
    if a.dependencies.is_empty() {
        let _ = writeln!(s, "_none_\n");
    } else {
        let _ = writeln!(s, "| Name | Version | Dev |");
        let _ = writeln!(s, "| --- | --- | --- |");
        for d in &a.dependencies {
            let _ = writeln!(s, "| {} | {} | {} |", d.name, d.version, d.dev);
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(s, "## Top-level layout\n");
    if a.top_dirs.is_empty() {
        let _ = writeln!(s, "_no subdirectories_");
    } else {
        for d in &a.top_dirs {
            let _ = writeln!(s, "- `{d}/`");
        }
    }
    s
}

/// Render and write the report to `.agent/reports/project-analysis.md`.
pub fn write_report(root: &Utf8Path, a: &ProjectAnalysis) -> Result<Utf8PathBuf> {
    let dir = root.join(".agent/reports");
    std::fs::create_dir_all(dir.as_std_path())?;
    let path = dir.join("project-analysis.md");
    std::fs::write(path.as_std_path(), render(a))?;
    Ok(path)
}

fn language_str(l: &LanguageKind) -> &'static str {
    match l {
        LanguageKind::Rust => "Rust",
        LanguageKind::JavaScript => "JavaScript",
        LanguageKind::TypeScript => "TypeScript",
        LanguageKind::Mixed => "Mixed",
        LanguageKind::Unknown => "Unknown",
    }
}

fn pm_str(p: &PackageManager) -> &'static str {
    match p {
        PackageManager::Cargo => "Cargo",
        PackageManager::Npm => "npm",
        PackageManager::Pnpm => "pnpm",
        PackageManager::Yarn => "Yarn",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::project_analysis::DependencySummary;

    fn sample(root: &Utf8Path) -> ProjectAnalysis {
        ProjectAnalysis {
            root: root.to_path_buf(),
            language: LanguageKind::Rust,
            package_manager: Some(PackageManager::Cargo),
            dependencies: vec![DependencySummary {
                name: "serde".into(),
                version: "1".into(),
                dev: false,
            }],
            file_count: 5,
            source_files: 3,
            top_dirs: vec!["crates".into()],
        }
    }

    #[test]
    fn renders_and_writes_report() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let a = sample(root);
        let path = write_report(root, &a).unwrap();
        let md = std::fs::read_to_string(path.as_std_path()).unwrap();
        assert!(md.starts_with("# Project Analysis"));
        assert!(md.contains("## Dependencies"));
        assert!(md.contains("serde"));
        assert!(root
            .join(".agent/reports/project-analysis.md")
            .as_std_path()
            .exists());
    }
}
