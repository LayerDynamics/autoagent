//! Project analyzer (M2) — language/package-manager detection and the
//! `analyze` assembler. PROPOSED DESIGN: detection heuristics below are an M2
//! decision (see the plan's open questions).

use crate::analysis::project_analysis::{LanguageKind, PackageManager};
use crate::error::Result;
use camino::Utf8Path;

/// Detect the project's primary language and package manager from manifests.
pub fn detect(root: &Utf8Path) -> Result<(LanguageKind, Option<PackageManager>)> {
    let has_cargo = root.join("Cargo.toml").as_std_path().exists();
    let has_pkg = root.join("package.json").as_std_path().exists();

    let language = match (has_cargo, has_pkg) {
        (true, true) => LanguageKind::Mixed,
        (true, false) => LanguageKind::Rust,
        (false, true) => {
            if root.join("tsconfig.json").as_std_path().exists() {
                LanguageKind::TypeScript
            } else {
                LanguageKind::JavaScript
            }
        }
        (false, false) => LanguageKind::Unknown,
    };

    let package_manager = if has_cargo {
        Some(PackageManager::Cargo)
    } else if has_pkg {
        if root.join("pnpm-lock.yaml").as_std_path().exists() {
            Some(PackageManager::Pnpm)
        } else if root.join("yarn.lock").as_std_path().exists() {
            Some(PackageManager::Yarn)
        } else {
            Some(PackageManager::Npm)
        }
    } else {
        None
    };

    Ok((language, package_manager))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_rust_by_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "fn main(){}").unwrap();
        let (lang, pm) = detect(root).unwrap();
        assert_eq!(lang, LanguageKind::Rust);
        assert_eq!(pm, Some(PackageManager::Cargo));
    }

    #[test]
    fn detects_npm_by_package_json_and_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"x","dependencies":{}}"#,
        )
        .unwrap();
        std::fs::write(root.join("package-lock.json"), "{}").unwrap();
        let (lang, pm) = detect(root).unwrap();
        assert_eq!(lang, LanguageKind::JavaScript);
        assert_eq!(pm, Some(PackageManager::Npm));
    }

    #[test]
    fn detects_typescript_and_pnpm() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("package.json"), r#"{"name":"x"}"#).unwrap();
        std::fs::write(root.join("tsconfig.json"), "{}").unwrap();
        std::fs::write(root.join("pnpm-lock.yaml"), "").unwrap();
        let (lang, pm) = detect(root).unwrap();
        assert_eq!(lang, LanguageKind::TypeScript);
        assert_eq!(pm, Some(PackageManager::Pnpm));
    }

    #[test]
    fn unknown_when_no_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let (lang, pm) = detect(root).unwrap();
        assert_eq!(lang, LanguageKind::Unknown);
        assert_eq!(pm, None);
    }
}
