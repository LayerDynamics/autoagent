//! Workspace file scanner honoring include/exclude globs (SPEC-1 FR-5).
//!
//! `standard_filters(false)` makes the walk deterministic; include/exclude
//! globs do all the filtering so behavior does not depend on ambient
//! `.gitignore` state above the workspace.

use crate::error::{AutoAgentError, Result};
use camino::{Utf8Path, Utf8PathBuf};
use globset::{Glob, GlobSet, GlobSetBuilder};

pub fn scan(root: &Utf8Path, include: &[String], exclude: &[String]) -> Result<Vec<Utf8PathBuf>> {
    let inc = build_set(include)?;
    let exc = build_set(exclude)?;
    let mut out = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .hidden(false)
        .build();
    for entry in walker {
        let entry = entry.map_err(|e| AutoAgentError::Analysis(e.to_string()))?;
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let abs = Utf8PathBuf::from_path_buf(entry.into_path())
            .map_err(|_| AutoAgentError::Analysis("non-utf8 path".into()))?;
        let rel = abs.strip_prefix(root).unwrap_or(&abs);
        if exc.is_match(rel.as_str()) {
            continue;
        }
        if inc.is_match(rel.as_str()) {
            out.push(rel.to_path_buf());
        }
    }
    out.sort();
    Ok(out)
}

fn build_set(globs: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for g in globs {
        b.add(Glob::new(g).map_err(|e| AutoAgentError::Analysis(e.to_string()))?);
    }
    b.build()
        .map_err(|e| AutoAgentError::Analysis(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_exclude() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "x").unwrap();
        std::fs::write(root.join("target/junk.rs"), "x").unwrap();
        let files = scan(root, &["**/*.rs".into()], &["target/**".into()]).unwrap();
        assert!(files.iter().any(|f| f.ends_with("src/lib.rs")));
        assert!(!files.iter().any(|f| f.as_str().contains("target")));
    }

    #[test]
    fn include_filters_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("a.rs"), "x").unwrap();
        std::fs::write(root.join("b.txt"), "x").unwrap();
        let files = scan(root, &["**/*.rs".into()], &[]).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("a.rs"));
    }
}
