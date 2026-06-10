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
// Approval bridge (B3-T1). Bridges core's ApprovalGate to a host callback,
// fail-closed: absent an affirmative decision, every privileged op is refused
// (FR-7 / FR-20). The mutating wrappers build a gate from the host's callback
// (or an explicit `approve` flag) and hand it to core, which routes every
// privileged step through it — bindings add no bypass.
// ---------------------------------------------------------------------------

use autoagent_core::error::PolicyError;
use autoagent_core::safety::approval_gate::ApprovalGate;

/// A privileged action presented to the host for approval.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    /// `"write"` or `"command"`.
    pub kind: String,
    /// The path being written or the command being run.
    pub target: String,
}

/// An `ApprovalGate` whose decision comes from a host-supplied callback. The
/// callback returns `true` to allow, `false` to refuse; if it is never called
/// affirmatively the action is refused with a `policy.*` error.
pub struct CallbackGate {
    cb: Box<dyn Fn(ApprovalRequest) -> bool + Send + Sync>,
}

impl CallbackGate {
    /// Build a gate from a host decision function.
    pub fn from_fn(f: impl Fn(ApprovalRequest) -> bool + Send + Sync + 'static) -> Self {
        Self { cb: Box::new(f) }
    }

    /// Unconditionally approve — used when the host passes an explicit
    /// `approve` / `auto_approve` flag (matches CLI `--yes`).
    pub fn approve_all() -> Self {
        Self::from_fn(|_| true)
    }

    /// Unconditionally refuse — the default when no callback and no approve flag
    /// is supplied (fail-closed).
    pub fn deny_all() -> Self {
        Self::from_fn(|_| false)
    }
}

impl ApprovalGate for CallbackGate {
    fn confirm_write(&self, target: &str) -> autoagent_core::error::Result<()> {
        if (self.cb)(ApprovalRequest {
            kind: "write".into(),
            target: target.into(),
        }) {
            Ok(())
        } else {
            Err(PolicyError::WriteNotApproved(target.into()).into())
        }
    }

    fn confirm_command(&self, command: &str) -> autoagent_core::error::Result<()> {
        if (self.cb)(ApprovalRequest {
            kind: "command".into(),
            target: command.into(),
        }) {
            Ok(())
        } else {
            Err(PolicyError::CommandNotApproved(command.into()).into())
        }
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

/// Initialize `Autoagent.toml` + the `.agent/` tree; returns `"true"` if files
/// were written, `"false"` if an existing config was preserved.
pub fn init(root: &str) -> BindResult {
    let wrote = autoagent_core::runtime::init::init_workspace(utf8(root)?)?;
    Ok(wrote.to_string())
}

/// Render the effective `Autoagent.toml`.
pub fn config_show(root: &str) -> BindResult {
    let cfg = autoagent_core::config::config_schema::AutoAgentConfig::load(utf8(root)?)?;
    toml::to_string_pretty(&cfg).map_err(|e| BindError {
        code: "config".into(),
        exit_code: 2,
        message: e.to_string(),
    })
}

/// List patch artifact run ids (files under `.agent/patches/*.patch`) as a JSON
/// array. Mirrors `commands::patch_list` but returns data instead of printing.
pub fn patch_list(root: &str) -> BindResult {
    let dir = utf8(root)?.join(".agent/patches");
    let mut names: Vec<String> = match std::fs::read_dir(dir.as_std_path()) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with(".patch"))
            .map(|n| n.trim_end_matches(".patch").to_string())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    serde_json::to_string(&names).map_err(serde_err)
}

/// Show a patch body for a run id. Mirrors `commands::patch_show`.
pub fn patch_show(root: &str, run_id: &str) -> BindResult {
    let path = utf8(root)?
        .join(".agent/patches")
        .join(format!("{run_id}.patch"));
    std::fs::read_to_string(path.as_std_path()).map_err(|_| BindError {
        code: "revert".into(),
        exit_code: 7,
        message: format!("no patch for run {run_id}"),
    })
}

/// Project memory summary (name, language, package manager, file count, decision
/// count) as a JSON object. Mirrors the data `commands::memory_show` prints.
pub fn memory_show(root: &str) -> BindResult {
    let root = utf8(root)?;
    let cfg = autoagent_core::config::config_schema::AutoAgentConfig::load(root)?;
    let store =
        autoagent_core::memory::memory_store::MemoryStore::new(root.join(&cfg.memory.directory));
    let pm = store.load_project()?;
    let decisions = store.load_decisions()?;
    let summary = autoagent_core::memory::summary::MemorySummary {
        name: pm.name,
        language: pm.language,
        package_manager: pm.package_manager,
        source_file_count: pm.source_file_count,
        decisions: decisions.len(),
    };
    serde_json::to_string(&summary).map_err(serde_err)
}

/// List registered plugin tools (builtins + discovered WASM plugins) as a JSON
/// array. Mirrors `commands::tools_list`.
pub fn tools_list(root: &str) -> BindResult {
    let mut names = autoagent_core::plugins::with_builtins()?.tool_names();
    for m in autoagent_core::plugins::discover_wasm_plugins(utf8(root)?) {
        names.push(m.name);
    }
    serde_json::to_string(&names).map_err(serde_err)
}

// ---------------------------------------------------------------------------
// Mutating wrappers (B3-T2). Every privileged op routes through core's
// PolicyEngine + snapshot + audit via a CallbackGate built from the host's
// approval decision. `approve == false` with no callback is fail-closed.
// ---------------------------------------------------------------------------

use autoagent_core::config::config_schema::{AutoAgentConfig, LlmConfig};

/// The default offline LLM config (matches `commands::run`/`evolve` fallback)
/// used when no `[llm]` block is configured.
fn default_local_llm() -> LlmConfig {
    LlmConfig {
        provider: "local".into(),
        model: "llama3".into(),
        endpoint: None,
        code_egress_opt_in: false,
    }
}

fn gate_for(approve: bool) -> CallbackGate {
    if approve {
        CallbackGate::approve_all()
    } else {
        CallbackGate::deny_all()
    }
}

/// Apply a plan through the policy-controlled mutation engine; returns the run
/// id. The gate is consulted before any write (fail-closed when `approve`
/// is false and the config requires approval).
pub fn apply(root: &str, plan_path: &str, approve: bool) -> BindResult {
    let gate = gate_for(approve);
    let run_id =
        autoagent_core::runtime::agent_loop::apply_with_gate(utf8(root)?, utf8(plan_path)?, &gate)?;
    Ok(run_id)
}

/// Revert a previous run, restoring the pre-run snapshot.
pub fn revert(root: &str, run_id: &str) -> BindResult {
    autoagent_core::runtime::revert::revert(utf8(root)?, run_id)?;
    Ok(String::new())
}

/// Supervised run (blocking): apply a plan (from `plan_path`) or generate one
/// via the configured LLM, validate, bounded-repair, and report. Returns a
/// serialized `RunOutcome`. Mirrors `commands::run`, including the up-front gate
/// check. The async backend variants wrap this on a blocking thread.
pub fn run_sync(root: &str, objective: &str, plan_path: Option<&str>, approve: bool) -> BindResult {
    let root = utf8(root)?;
    let config = AutoAgentConfig::load(root)?;

    // Resolve the write/command approval decision once for the whole run, as the
    // CLI does. When `approve` is false this refuses fail-closed.
    let gate = gate_for(approve);
    if config.agent.require_approval_before_write && !approve {
        gate.confirm_write("planned changes")
            .map_err(BindError::from)?;
    }
    if config.agent.require_approval_before_command && !approve {
        gate.confirm_command("validation commands")
            .map_err(BindError::from)?;
    }

    let outcome = if let Some(p) = plan_path {
        autoagent_core::runtime::run_workflow::run_with_plan(root, utf8(p)?, true)?
    } else {
        let llm = config.llm.clone().unwrap_or_else(default_local_llm);
        let provider = autoagent_core::planning::llm::config::build_provider(&llm)?;
        let rt = tokio::runtime::Runtime::new().map_err(|e| BindError {
            code: "io".into(),
            exit_code: 1,
            message: e.to_string(),
        })?;
        rt.block_on(autoagent_core::runtime::run_workflow::run_workflow(
            root,
            objective,
            provider.as_ref(),
            true,
        ))?
    };
    serde_json::to_string(&outcome).map_err(serde_err)
}

/// Controlled self-authoring (blocking): generate (or import via `plan_path`) a
/// self-plan, plan-only unless `apply` is set (which is itself gated by
/// `allow_self_modification` in core). Returns a serialized `EvolveOutcome`.
/// The async backend variants wrap this on a blocking thread.
pub fn evolve_sync(
    root: &str,
    objective: &str,
    plan_path: Option<&str>,
    apply: bool,
) -> BindResult {
    let root = utf8(root)?;
    let config = AutoAgentConfig::load(root)?;

    let outcome = if let Some(p) = plan_path {
        let plan = autoagent_core::planning::plan_reader::read_plan(utf8(p)?)?;
        autoagent_core::runtime::evolve_workflow::evolve_with_plan(root, objective, &plan, apply)?
    } else {
        let llm = config.llm.clone().unwrap_or_else(default_local_llm);
        let provider = autoagent_core::planning::llm::config::build_provider(&llm)?;
        let rt = tokio::runtime::Runtime::new().map_err(|e| BindError {
            code: "io".into(),
            exit_code: 1,
            message: e.to_string(),
        })?;
        rt.block_on(autoagent_core::runtime::evolve_workflow::evolve_generated(
            root,
            objective,
            provider.as_ref(),
            apply,
        ))?
    };
    serde_json::to_string(&outcome).map_err(serde_err)
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
                name: "approve",
                ty: "boolean",
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
                name: "approve",
                ty: "boolean",
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
    // Blocking variants of the async run/evolve workflows (FR-5 `*_sync`).
    Symbol {
        name: "run_sync",
        kind: Kind::Sync,
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
                name: "approve",
                ty: "boolean",
            },
        ],
        returns: "RunOutcome",
        doc: "Supervised run (blocking) from a plan or generated objective.",
    },
    Symbol {
        name: "evolve_sync",
        kind: Kind::Sync,
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
        doc: "Controlled self-authoring (blocking).",
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
