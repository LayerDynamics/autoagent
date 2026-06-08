//! Snapshot manager — copies files into the run folder's `before/` and `after/`
//! directories and computes sha256 content hashes (SPEC-1 FR-7, §3.8).

use crate::error::Result;
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

pub struct SnapshotManager {
    run_dir: Utf8PathBuf,
}

impl SnapshotManager {
    pub fn new(run_dir: Utf8PathBuf) -> Self {
        Self { run_dir }
    }

    /// Copy `<root>/<rel>` into `<run_dir>/before/<rel>`; return its sha256.
    pub fn snapshot(&self, root: &Utf8Path, rel: Utf8PathBuf) -> Result<String> {
        self.copy_into("before", root, rel)
    }

    /// Copy the post-mutation `<root>/<rel>` into `<run_dir>/after/<rel>`.
    pub fn record_after(&self, root: &Utf8Path, rel: Utf8PathBuf) -> Result<String> {
        self.copy_into("after", root, rel)
    }

    fn copy_into(&self, sub: &str, root: &Utf8Path, rel: Utf8PathBuf) -> Result<String> {
        let src = root.join(&rel);
        let bytes = std::fs::read(src.as_std_path())?;
        let dst = self.run_dir.join(sub).join(&rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent.as_std_path())?;
        }
        std::fs::write(dst.as_std_path(), &bytes)?;
        Ok(sha256_hex(&bytes))
    }
}

/// Lowercase-hex sha256 of `bytes` (the canonical content hash).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_and_hashes_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("a.txt"), b"hello").unwrap();
        let run = root.join("run");
        let mgr = SnapshotManager::new(run.clone());
        let hash = mgr.snapshot(root, "a.txt".into()).unwrap();
        assert_eq!(hash, sha256_hex(b"hello"));
        assert!(run.join("before/a.txt").as_std_path().exists());
    }

    #[test]
    fn snapshot_of_missing_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let mgr = SnapshotManager::new(root.join("run"));
        assert!(mgr.snapshot(root, "nope.txt".into()).is_err());
    }

    #[test]
    fn sha256_is_stable() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
