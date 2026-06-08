//! File editor — applies a single `FileOperation` to the workspace
//! (SPEC-1 §3.5 step 11). Path policy is enforced upstream by the validator;
//! this performs the actual mutation.

use crate::editing::file_operation::{FileOperation, FileOperationKind};
use crate::error::{AutoAgentError, Result};
use camino::Utf8PathBuf;

pub struct FileEditor {
    root: Utf8PathBuf,
}

impl FileEditor {
    pub fn new(root: Utf8PathBuf) -> Self {
        Self { root }
    }

    pub fn apply(&self, op: &FileOperation) -> Result<()> {
        use FileOperationKind::*;
        let abs = self.root.join(&op.path);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent.as_std_path())?;
        }
        match op.kind {
            Create | Write | Replace => {
                let c = op
                    .content
                    .as_ref()
                    .ok_or_else(|| AutoAgentError::Editing("missing content".into()))?;
                std::fs::write(abs.as_std_path(), c)?;
            }
            Append => {
                use std::io::Write;
                let c = op
                    .content
                    .as_ref()
                    .ok_or_else(|| AutoAgentError::Editing("missing content".into()))?;
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(abs.as_std_path())?;
                f.write_all(c.as_bytes())?;
            }
            Delete => {
                std::fs::remove_file(abs.as_std_path())?;
            }
            Rename => {
                let dest = op
                    .destination_path
                    .as_ref()
                    .ok_or_else(|| AutoAgentError::Editing("rename missing destination".into()))?;
                let dest_abs = self.root.join(dest);
                if let Some(parent) = dest_abs.parent() {
                    std::fs::create_dir_all(parent.as_std_path())?;
                }
                std::fs::rename(abs.as_std_path(), dest_abs.as_std_path())?;
            }
            CreateDirectory => {
                std::fs::create_dir_all(abs.as_std_path())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::file_operation::FileOperationKind;
    use camino::Utf8PathBuf;

    fn op(kind: FileOperationKind, path: &str, content: Option<&str>) -> FileOperation {
        FileOperation {
            kind,
            path: Utf8PathBuf::from(path),
            destination_path: None,
            reason: "r".into(),
            before_hash: None,
            after_hash: None,
            content: content.map(|c| c.to_string()),
        }
    }

    #[test]
    fn applies_create_then_replace() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let ed = FileEditor::new(root.to_path_buf());
        ed.apply(&op(FileOperationKind::Create, "a.txt", Some("v1")))
            .unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v1");
        ed.apply(&op(FileOperationKind::Replace, "a.txt", Some("v2")))
            .unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2");
    }

    #[test]
    fn append_and_delete_and_rename() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let ed = FileEditor::new(root.to_path_buf());
        ed.apply(&op(FileOperationKind::Create, "log.txt", Some("a")))
            .unwrap();
        ed.apply(&op(FileOperationKind::Append, "log.txt", Some("b")))
            .unwrap();
        assert_eq!(std::fs::read_to_string(root.join("log.txt")).unwrap(), "ab");

        let mut rename = op(FileOperationKind::Rename, "log.txt", None);
        rename.destination_path = Some("renamed.txt".into());
        ed.apply(&rename).unwrap();
        assert!(root.join("renamed.txt").as_std_path().exists());
        assert!(!root.join("log.txt").as_std_path().exists());

        ed.apply(&op(FileOperationKind::Delete, "renamed.txt", None))
            .unwrap();
        assert!(!root.join("renamed.txt").as_std_path().exists());
    }
}
