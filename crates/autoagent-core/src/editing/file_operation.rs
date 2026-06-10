//! The unit of mutation (SPEC-1 §3.3 / §8.2).

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileOperationKind {
    Create,
    Write,
    Replace,
    Append,
    Delete,
    Rename,
    CreateDirectory,
    /// Surgical in-place edit: replace the single, unique occurrence of `anchor`
    /// with `content`. The preferred way to edit an existing file — it never
    /// rewrites the whole file, so it cannot truncate or hallucinate the rest.
    Substitute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOperation {
    pub kind: FileOperationKind,
    pub path: Utf8PathBuf,
    pub destination_path: Option<Utf8PathBuf>,
    pub reason: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub content: Option<String>,
    /// For `Substitute`: the exact existing text to find (must occur exactly
    /// once). Ignored by other kinds.
    #[serde(default)]
    pub anchor: Option<String>,
}
