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
            Substitute => {
                let anchor = op
                    .anchor
                    .as_ref()
                    .ok_or_else(|| AutoAgentError::Editing("substitute missing anchor".into()))?;
                let content = op
                    .content
                    .as_ref()
                    .ok_or_else(|| AutoAgentError::Editing("substitute missing content".into()))?;
                let current = std::fs::read_to_string(abs.as_std_path()).map_err(|e| {
                    AutoAgentError::Editing(format!("substitute target {}: {e}", op.path))
                })?;
                // Require a UNIQUE anchor so the edit is unambiguous and safe.
                let matches = current.matches(anchor.as_str()).count();
                if matches == 0 {
                    return Err(AutoAgentError::Editing(format!(
                        "substitute anchor not found in {}",
                        op.path
                    )));
                }
                if matches > 1 {
                    return Err(AutoAgentError::Editing(format!(
                        "substitute anchor is not unique ({matches} matches) in {} — include more surrounding context",
                        op.path
                    )));
                }
                let updated = current.replacen(anchor.as_str(), content, 1);
                std::fs::write(abs.as_std_path(), updated)?;
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
            anchor: None,
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

    #[test]
    fn substitute_edits_unique_anchor_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let ed = FileEditor::new(root.to_path_buf());
        ed.apply(&op(
            FileOperationKind::Create,
            "lib.rs",
            Some("pub mod a;\npub mod b;\n"),
        ))
        .unwrap();

        // Surgical edit: replace exactly one unique anchor, leave the rest intact.
        let mut sub = op(
            FileOperationKind::Substitute,
            "lib.rs",
            Some("pub mod b;\npub mod c;"),
        );
        sub.anchor = Some("pub mod b;".into());
        ed.apply(&sub).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("lib.rs")).unwrap(),
            "pub mod a;\npub mod b;\npub mod c;\n"
        );
    }

    #[test]
    fn substitute_rejects_missing_and_ambiguous_anchor() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let ed = FileEditor::new(root.to_path_buf());
        ed.apply(&op(FileOperationKind::Create, "f.rs", Some("x\nx\n")))
            .unwrap();

        // Anchor not present -> error, file untouched.
        let mut missing = op(FileOperationKind::Substitute, "f.rs", Some("y"));
        missing.anchor = Some("zzz".into());
        assert!(ed.apply(&missing).is_err());

        // Anchor not unique (two "x") -> error rather than an ambiguous edit.
        let mut ambiguous = op(FileOperationKind::Substitute, "f.rs", Some("y"));
        ambiguous.anchor = Some("x".into());
        assert!(ed.apply(&ambiguous).is_err());

        assert_eq!(
            std::fs::read_to_string(root.join("f.rs")).unwrap(),
            "x\nx\n"
        );
    }

    #[test]
    fn content_ops_without_content_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let ed = FileEditor::new(root.to_path_buf());
        // Create/Write/Replace/Append all require `content`; a malformed op must
        // error cleanly and write nothing — never panic or truncate.
        for kind in [
            FileOperationKind::Create,
            FileOperationKind::Write,
            FileOperationKind::Replace,
            FileOperationKind::Append,
        ] {
            let label = format!("{kind:?}");
            assert!(ed.apply(&op(kind, "a.txt", None)).is_err(), "{label}");
        }
        assert!(!root.join("a.txt").as_std_path().exists());
    }

    #[test]
    fn append_creates_the_file_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let ed = FileEditor::new(root.to_path_buf());
        // Append to a not-yet-existing file creates it (OpenOptions::create(true)).
        ed.apply(&op(FileOperationKind::Append, "new.txt", Some("hello")))
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("new.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn rename_without_destination_errors() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let ed = FileEditor::new(root.to_path_buf());
        ed.apply(&op(FileOperationKind::Create, "a.txt", Some("v")))
            .unwrap();
        // `op()` leaves destination_path None.
        assert!(ed
            .apply(&op(FileOperationKind::Rename, "a.txt", None))
            .is_err());
        assert!(
            root.join("a.txt").as_std_path().exists(),
            "source preserved"
        );
    }

    #[test]
    fn substitute_without_anchor_field_or_content_errors() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let ed = FileEditor::new(root.to_path_buf());
        ed.apply(&op(
            FileOperationKind::Create,
            "f.rs",
            Some("anchor here\n"),
        ))
        .unwrap();

        // Anchor field absent (None) -> error.
        assert!(ed
            .apply(&op(FileOperationKind::Substitute, "f.rs", Some("x")))
            .is_err());

        // Anchor present but no replacement content -> error.
        let mut no_content = op(FileOperationKind::Substitute, "f.rs", None);
        no_content.anchor = Some("anchor here".into());
        assert!(ed.apply(&no_content).is_err());

        // The target is untouched after either refusal.
        assert_eq!(
            std::fs::read_to_string(root.join("f.rs")).unwrap(),
            "anchor here\n"
        );
    }

    #[test]
    fn create_directory_makes_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let ed = FileEditor::new(root.to_path_buf());
        ed.apply(&op(FileOperationKind::CreateDirectory, "a/b/c", None))
            .unwrap();
        assert!(root.join("a/b/c").as_std_path().is_dir());
    }
}
