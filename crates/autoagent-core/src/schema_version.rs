//! Schema versioning (M8, SPEC-1 §3.4) — the frozen on-disk contract version.
//! A 1.0 binary refuses artifacts that declare a newer schema than it supports.

use crate::error::{AutoAgentError, Result};

/// The plan/run/event schema version this build implements (frozen at 1.0.0).
pub const SCHEMA_VERSION: u32 = 1;

/// Accept an artifact's declared schema version, or reject if it is newer.
pub fn accepts_version(v: u32) -> Result<()> {
    if v <= SCHEMA_VERSION {
        Ok(())
    } else {
        Err(AutoAgentError::Plan(format!(
            "artifact schema_version {v} > supported {SCHEMA_VERSION}; upgrade AutoAgent"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_is_one() {
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn accepts_current_and_older() {
        assert!(accepts_version(1).is_ok());
        assert!(accepts_version(0).is_ok());
    }

    #[test]
    fn rejects_future_version() {
        let e = accepts_version(2).unwrap_err();
        assert_eq!(e.error_code(), "plan");
    }
}
