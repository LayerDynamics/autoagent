//! The error taxonomy for autoagent-core (SPEC-1 §3.11).
//!
//! `AutoAgentError` is the crate-wide error. Policy *refusals* are a distinct
//! sub-enum (`PolicyError`) so a denial is never confused with a crash. Each
//! variant maps to a stable process exit code and a machine `error_code`.

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, AutoAgentError>;

#[derive(Debug, thiserror::Error)]
pub enum AutoAgentError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("workspace error: {0}")]
    Workspace(String),
    #[error("analysis error: {0}")]
    Analysis(String),
    #[error("plan error: {0}")]
    Plan(String),
    #[error("policy denied: {0}")]
    Policy(#[from] PolicyError),
    #[error("editing error: {0}")]
    Editing(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("revert error: {0}")]
    Revert(String),
    #[error("memory error: {0}")]
    Memory(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(String),
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("path escapes workspace: {0}")]
    PathEscape(String),
    #[error("symlink target escapes workspace: {0}")]
    SymlinkEscape(String),
    #[error("path is blocked by policy: {0}")]
    BlockedPath(String),
    #[error("path not in allowed write paths: {0}")]
    NotAllowed(String),
    #[error("command is blocked by policy: {0}")]
    BlockedCommand(String),
    #[error("unsafe shell syntax: {0}")]
    UnsafeShellSyntax(String),
    #[error("command requires approval and was denied: {0}")]
    CommandNotApproved(String),
    #[error("write requires approval and was denied: {0}")]
    WriteNotApproved(String),
}

impl AutoAgentError {
    /// Stable process exit code (SPEC-1 §3.11 mapping table).
    pub fn exit_code(&self) -> i32 {
        match self {
            AutoAgentError::Config(_) | AutoAgentError::Workspace(_) => 2,
            AutoAgentError::Plan(_) => 3,
            AutoAgentError::Policy(_) => 4,
            AutoAgentError::Editing(_) => 5,
            AutoAgentError::Validation(_) => 6,
            AutoAgentError::Revert(_) => 7,
            AutoAgentError::Memory(_) => 8,
            AutoAgentError::Io(_) | AutoAgentError::Serde(_) | AutoAgentError::Analysis(_) => 1,
        }
    }

    /// Stable machine-readable error code, recorded in events/run.json.
    pub fn error_code(&self) -> String {
        match self {
            AutoAgentError::Policy(p) => format!("policy.{}", p.code()),
            AutoAgentError::Config(_) => "config".into(),
            AutoAgentError::Workspace(_) => "workspace".into(),
            AutoAgentError::Analysis(_) => "analysis".into(),
            AutoAgentError::Plan(_) => "plan".into(),
            AutoAgentError::Editing(_) => "editing".into(),
            AutoAgentError::Validation(_) => "validation".into(),
            AutoAgentError::Revert(_) => "revert".into(),
            AutoAgentError::Memory(_) => "memory".into(),
            AutoAgentError::Io(_) => "io".into(),
            AutoAgentError::Serde(_) => "serde".into(),
        }
    }
}

impl PolicyError {
    /// Short stable sub-code (used as `policy.<code>`).
    pub fn code(&self) -> &'static str {
        match self {
            PolicyError::PathEscape(_) => "path_escape",
            PolicyError::SymlinkEscape(_) => "symlink_escape",
            PolicyError::BlockedPath(_) => "blocked_path",
            PolicyError::NotAllowed(_) => "not_allowed",
            PolicyError::BlockedCommand(_) => "blocked_command",
            PolicyError::UnsafeShellSyntax(_) => "unsafe_shell_syntax",
            PolicyError::CommandNotApproved(_) => "command_not_approved",
            PolicyError::WriteNotApproved(_) => "write_not_approved",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_error_maps_to_exit_code_4() {
        let e: AutoAgentError = PolicyError::BlockedPath(".git/config".into()).into();
        assert_eq!(e.exit_code(), 4);
        assert_eq!(e.error_code(), "policy.blocked_path");
    }

    #[test]
    fn validation_error_is_exit_6() {
        assert_eq!(
            AutoAgentError::Validation("cargo test".into()).exit_code(),
            6
        );
    }

    #[test]
    fn io_error_maps_to_exit_1_and_io_code() {
        let e: AutoAgentError = std::io::Error::new(std::io::ErrorKind::NotFound, "x").into();
        assert_eq!(e.exit_code(), 1);
        assert_eq!(e.error_code(), "io");
    }
}
