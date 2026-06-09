//! Surface registry + backend-neutral wrappers — the single source of truth.
//!
//! The registry (`SURFACE`) is the contract; `main.rs` reads it to generate every
//! backend adapter, the type stubs, the JSON schema, and the package scaffolds.
//! The neutral wrappers (B1-T5+) call `autoagent-core` and marshal results as
//! JSON; the `CallbackGate` (B3-T1) bridges core's approval gate to a host
//! callback.

/// Whether a symbol blocks (`Sync`) or returns a future/promise (`Async`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Sync,
    Async,
}

/// Whether a symbol only reads (`Read`) or can mutate the workspace (`Mutate`).
/// Mutating symbols always route through core's PolicyEngine + snapshot + audit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Privilege {
    Read,
    Mutate,
}

/// One argument of an exported symbol, with its cross-language type name.
#[derive(Clone, Copy, Debug)]
pub struct Arg {
    pub name: &'static str,
    /// TypeScript/Python type rendered into stubs and schema (e.g. `"string"`).
    pub ty: &'static str,
}

/// One exported symbol of the bound surface.
#[derive(Clone, Copy, Debug)]
pub struct Symbol {
    pub name: &'static str,
    pub kind: Kind,
    pub privilege: Privilege,
    pub args: &'static [Arg],
    /// Return type name (TS/Python), or `"void"`.
    pub returns: &'static str,
    pub doc: &'static str,
}

const S_ROOT: Arg = Arg {
    name: "root",
    ty: "string",
};

// ---------------------------------------------------------------------------
// Backend-neutral error + result.
// ---------------------------------------------------------------------------

use autoagent_core::error::AutoAgentError;
use camino::Utf8Path;

/// Backend-neutral error carrying the stable code + numeric exit code from
/// core's taxonomy, so every host runtime can branch on the same categories
/// (e.g. `policy.path_escape`) the CLI uses (FR-8).
#[derive(Debug, Clone, serde::Serialize)]
pub struct BindError {
    pub code: String,
    pub exit_code: i32,
    pub message: String,
}

impl From<AutoAgentError> for BindError {
    fn from(e: AutoAgentError) -> Self {
        BindError {
            code: e.error_code(),
            exit_code: e.exit_code(),
            message: e.to_string(),
        }
    }
}

/// Every neutral wrapper returns a JSON string on success or a `BindError`.
pub type BindResult = std::result::Result<String, BindError>;

fn utf8(root: &str) -> std::result::Result<&Utf8Path, BindError> {
    Utf8Path::from_path(std::path::Path::new(root)).ok_or_else(|| BindError {
        code: "workspace".into(),
        exit_code: 2,
        message: "non-utf8 path".into(),
    })
}

fn serde_err(e: serde_json::Error) -> BindError {
    BindError {
        code: "serde".into(),
        exit_code: 1,
        message: e.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Neutral read wrappers (B1-T5). Each calls autoagent-core and returns JSON.
// ---------------------------------------------------------------------------

/// Schema version this build supports (rendered as a JSON number string).
pub fn version() -> BindResult {
    Ok(autoagent_core::schema_version::SCHEMA_VERSION.to_string())
}

/// Read-only health checks; returns a serialized `DoctorReport`.
pub fn doctor(root: &str) -> BindResult {
    let report = autoagent_core::runtime::doctor::doctor(utf8(root)?);
    serde_json::to_string(&report).map_err(serde_err)
}

/// Analyze the project (and write its report); returns a serialized
/// `ProjectAnalysis`.
pub fn analyze(root: &str) -> BindResult {
    let root = utf8(root)?;
    let cfg = autoagent_core::config::config_schema::AutoAgentConfig::load(root)?;
    let analysis = autoagent_core::analysis::project_analyzer::analyze(root, &cfg)?;
    autoagent_core::analysis::report_writer::write_report(root, &analysis)?;
    serde_json::to_string(&analysis).map_err(serde_err)
}

/// The single source of truth for the bound surface (full CLI parity, FR-4).
/// Every backend adapter and stub is generated from this table.
pub static SURFACE: &[Symbol] = &[
    Symbol {
        name: "version",
        kind: Kind::Sync,
        privilege: Privilege::Read,
        args: &[],
        returns: "number",
        doc: "Schema version this build supports.",
    },
    Symbol {
        name: "doctor",
        kind: Kind::Sync,
        privilege: Privilege::Read,
        args: &[S_ROOT],
        returns: "DoctorReport",
        doc: "System, config, and workspace health checks.",
    },
    Symbol {
        name: "analyze",
        kind: Kind::Sync,
        privilege: Privilege::Read,
        args: &[S_ROOT],
        returns: "ProjectAnalysis",
        doc: "Analyze the project and write the report.",
    },
    Symbol {
        name: "init",
        kind: Kind::Sync,
        privilege: Privilege::Mutate,
        args: &[S_ROOT],
        returns: "boolean",
        doc: "Initialize Autoagent.toml and the .agent/ tree.",
    },
    Symbol {
        name: "plan",
        kind: Kind::Async,
        privilege: Privilege::Read,
        args: &[
            S_ROOT,
            Arg {
                name: "objective",
                ty: "string",
            },
            Arg {
                name: "from",
                ty: "string | null",
            },
        ],
        returns: "string",
        doc: "Generate or import+validate a plan; returns the plan path.",
    },
    Symbol {
        name: "apply",
        kind: Kind::Sync,
        privilege: Privilege::Mutate,
        args: &[
            S_ROOT,
            Arg {
                name: "plan_path",
                ty: "string",
            },
            Arg {
                name: "opts",
                ty: "ApproveOpts",
            },
        ],
        returns: "string",
        doc: "Apply a plan through the policy engine; returns the run id.",
    },
    Symbol {
        name: "run",
        kind: Kind::Async,
        privilege: Privilege::Mutate,
        args: &[
            S_ROOT,
            Arg {
                name: "objective",
                ty: "string",
            },
            Arg {
                name: "from",
                ty: "string | null",
            },
            Arg {
                name: "opts",
                ty: "ApproveOpts",
            },
        ],
        returns: "RunOutcome",
        doc: "Supervised plan -> apply -> validate -> repair -> report.",
    },
    Symbol {
        name: "evolve",
        kind: Kind::Async,
        privilege: Privilege::Mutate,
        args: &[
            S_ROOT,
            Arg {
                name: "objective",
                ty: "string",
            },
            Arg {
                name: "from",
                ty: "string | null",
            },
            Arg {
                name: "apply",
                ty: "boolean",
            },
        ],
        returns: "EvolveOutcome",
        doc: "Controlled self-authoring plan (apply is policy-gated).",
    },
    Symbol {
        name: "revert",
        kind: Kind::Sync,
        privilege: Privilege::Mutate,
        args: &[
            S_ROOT,
            Arg {
                name: "run_id",
                ty: "string",
            },
        ],
        returns: "void",
        doc: "Revert a previous run.",
    },
    Symbol {
        name: "patch_list",
        kind: Kind::Sync,
        privilege: Privilege::Read,
        args: &[S_ROOT],
        returns: "string[]",
        doc: "List patch artifact run ids.",
    },
    Symbol {
        name: "patch_show",
        kind: Kind::Sync,
        privilege: Privilege::Read,
        args: &[
            S_ROOT,
            Arg {
                name: "run_id",
                ty: "string",
            },
        ],
        returns: "string",
        doc: "Show a patch body.",
    },
    Symbol {
        name: "config_show",
        kind: Kind::Sync,
        privilege: Privilege::Read,
        args: &[S_ROOT],
        returns: "string",
        doc: "Render Autoagent.toml.",
    },
    Symbol {
        name: "memory_show",
        kind: Kind::Sync,
        privilege: Privilege::Read,
        args: &[S_ROOT],
        returns: "MemorySummary",
        doc: "Project memory summary.",
    },
    Symbol {
        name: "tools_list",
        kind: Kind::Sync,
        privilege: Privilege::Read,
        args: &[S_ROOT],
        returns: "string[]",
        doc: "Registered plugin tools.",
    },
];
