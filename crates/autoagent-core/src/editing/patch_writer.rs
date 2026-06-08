//! Patch writer — concatenates per-file unified diffs into
//! `<patches_dir>/<run-id>.patch` (SPEC-1 §3.3 patch entity, FR-16).

use crate::editing::diff_builder;
use crate::error::Result;
use camino::{Utf8Path, Utf8PathBuf};

/// One file's before/after content for the patch.
pub struct PatchEntry {
    pub path: String,
    pub before: String,
    pub after: String,
}

/// Write the run's patch file and return its path.
pub fn write_patch(
    patches_dir: &Utf8Path,
    run_id: &str,
    entries: &[PatchEntry],
) -> Result<Utf8PathBuf> {
    std::fs::create_dir_all(patches_dir.as_std_path())?;
    let mut body = String::new();
    for e in entries {
        body.push_str(&diff_builder::unified(&e.before, &e.after, &e.path));
    }
    let path = patches_dir.join(format!("{run_id}.patch"));
    std::fs::write(path.as_std_path(), body)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_combined_patch() {
        let dir = tempfile::tempdir().unwrap();
        let patches = camino::Utf8Path::from_path(dir.path())
            .unwrap()
            .join("patches");
        let entries = vec![PatchEntry {
            path: "a.txt".into(),
            before: "old\n".into(),
            after: "new\n".into(),
        }];
        let p = write_patch(&patches, "run-1", &entries).unwrap();
        assert!(p.as_str().ends_with("run-1.patch"));
        let body = std::fs::read_to_string(p.as_std_path()).unwrap();
        assert!(body.contains("-old"));
        assert!(body.contains("+new"));
    }
}
