//! Source-tree summary helpers (M2).

use camino::Utf8Path;

const SKIP_DIRS: &[&str] = &[".agent", ".git", "target", "node_modules"];

/// First-level directories under `root`, excluding build/VCS/agent dirs, sorted.
pub fn top_dirs(root: &Utf8Path) -> Vec<String> {
    let mut dirs: Vec<String> = match std::fs::read_dir(root.as_std_path()) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !SKIP_DIRS.contains(&n.as_str()))
            .collect(),
        Err(_) => Vec::new(),
    };
    dirs.sort();
    dirs
}

/// True if a path looks like source (Rust / JS / TS).
pub fn is_source_file(path: &Utf8Path) -> bool {
    matches!(
        path.extension(),
        Some("rs") | Some("ts") | Some("tsx") | Some("js") | Some("jsx")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_dirs_excludes_build_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("crates")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let dirs = top_dirs(root);
        assert!(dirs.contains(&"crates".to_string()));
        assert!(!dirs.iter().any(|d| d == "target" || d == ".git"));
    }

    #[test]
    fn source_detection() {
        assert!(is_source_file(camino::Utf8Path::new("a/b.rs")));
        assert!(is_source_file(camino::Utf8Path::new("a/b.ts")));
        assert!(!is_source_file(camino::Utf8Path::new("a/b.md")));
    }
}
