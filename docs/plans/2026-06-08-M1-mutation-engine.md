# M1 — 0.1.0 Mutation Engine Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `lore:execute` to implement this plan task-by-task.
> **Scope guard:** Do ONLY what is listed here. If you discover adjacent issues, note them as a TODO and continue. Do NOT fix them.

**Goal:** Ship a safe, reversible mutation engine that applies user-supplied JSON plans through policy validation, snapshots, and a full audit trail.
**Architecture:** Cargo workspace with `autoagent-cli` (thin) and `autoagent-core` (all privileged logic). Every file write and command passes the `safety` module. Each run produces a complete, reversible run folder under `.agent/runs/<run-id>/`.
**Tech Stack:** Rust 2021, clap, serde/serde_json, toml, walkdir, ignore, globset, similar, sha2, camino, uuid, chrono, anyhow, thiserror; proptest (dev) for guard fuzzing.
**Practices:** TDD (failing test first), typed-interfaces-first, contract-first.
**Required skills:** none. proptest used for path/command guard property tests.
**Prerequisite:** none (this is the foundation milestone).
**Design status:** **Extracted from SPEC-1** (§3.3 data model, §3.4 schemas, §3.7 guard algorithms, §3.11 error model, §11 MVP scope). Contracts defined here are referenced unchanged by M2–M8.

**Canonical contracts established in this milestone (do NOT redefine downstream):** `RunState`, `AgentMode`, `TaskContext`, `FileOperationKind`, `FileOperation`, `Plan`, `PlannedFile`, `ValidationReport`, `CommandValidationResult`, `AutoAgentConfig`, `AutoAgentError`, `PolicyError`, the `events.jsonl` event catalog, and the `run.json` schema (SPEC-1 §3.4.2).

---

### Task 1: Workspace scaffold + git init

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `.gitignore`
- Create: `rust-toolchain.toml`
- Create: `crates/autoagent-core/Cargo.toml`
- Create: `crates/autoagent-cli/Cargo.toml`
- Create: `crates/autoagent-core/src/lib.rs`
- Create: `crates/autoagent-cli/src/main.rs`

**Step 1: Root `Cargo.toml`**
```toml
[workspace]
resolver = "2"
members = ["crates/autoagent-core", "crates/autoagent-cli"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
rust-version = "1.78"

[workspace.dependencies]
anyhow = "1"
thiserror = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
walkdir = "2"
ignore = "0.4"
globset = "0.4"
similar = "2"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
sha2 = "0.10"
camino = { version = "1", features = ["serde1"] }
console = "0.15"
dialoguer = "0.11"
indicatif = "0.17"
proptest = "1"
tempfile = "3"
```

**Step 2: `crates/autoagent-core/Cargo.toml`**
```toml
[package]
name = "autoagent-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
thiserror.workspace = true
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
walkdir.workspace = true
ignore.workspace = true
globset.workspace = true
similar.workspace = true
chrono.workspace = true
uuid.workspace = true
sha2.workspace = true
camino.workspace = true

[dev-dependencies]
proptest.workspace = true
tempfile.workspace = true
```

**Step 3: `crates/autoagent-cli/Cargo.toml`**
```toml
[package]
name = "autoagent-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "autoagent"
path = "src/main.rs"

[dependencies]
autoagent-core = { path = "../autoagent-core" }
anyhow.workspace = true
clap.workspace = true
console.workspace = true
dialoguer.workspace = true
indicatif.workspace = true
serde_json.workspace = true
```

**Step 4: `.gitignore`**
```gitignore
/target
.agent/runs/
.agent/patches/
.agent/logs/
.env
.env.*
```

**Step 5: `rust-toolchain.toml`**
```toml
[toolchain]
channel = "stable"
components = ["clippy", "rustfmt"]
```

**Step 6: stub entry points (compile-only, replaced in later tasks)**
`crates/autoagent-core/src/lib.rs`:
```rust
pub mod error;
```
`crates/autoagent-cli/src/main.rs`:
```rust
fn main() {
    println!("autoagent 0.1.0");
}
```

**Step 7: init repo and verify it builds**
`git init && cargo build` → Expected: workspace compiles (after Task 2 adds `error`).
> Note: `lib.rs` references `mod error` created in Task 2; run `cargo build` at the end of Task 2, not here. Here run `git init` only.

**Step 8: Commit**
`git add -A && git commit -m "chore: scaffold cargo workspace and git repo"`

---

### Task 2: Error taxonomy (contract-first, typed-first)

**Files:**
- Create: `crates/autoagent-core/src/error/mod.rs`
- Create: `crates/autoagent-core/src/error/autoagent_error.rs`
- Test: inline `#[cfg(test)]` in `autoagent_error.rs`

**Step 1: Write the failing test** (`autoagent_error.rs` bottom)
```rust
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
        assert_eq!(AutoAgentError::Validation("cargo test".into()).exit_code(), 6);
    }
}
```

**Step 2: Run to verify it fails**
`cargo test -p autoagent-core error` → Expected: FAIL (type/methods absent)

**Step 3: Write minimal implementation**
`error/autoagent_error.rs` — the full enums from SPEC-1 §3.11 plus the mapping methods:
```rust
#[derive(Debug, thiserror::Error)]
pub enum AutoAgentError {
    #[error("configuration error: {0}")] Config(String),
    #[error("workspace error: {0}")] Workspace(String),
    #[error("analysis error: {0}")] Analysis(String),
    #[error("plan error: {0}")] Plan(String),
    #[error("policy denied: {0}")] Policy(#[from] PolicyError),
    #[error("editing error: {0}")] Editing(String),
    #[error("validation failed: {0}")] Validation(String),
    #[error("revert error: {0}")] Revert(String),
    #[error("memory error: {0}")] Memory(String),
    #[error("io error: {0}")] Io(#[from] std::io::Error),
    #[error("serialization error: {0}")] Serde(String),
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("path escapes workspace: {0}")] PathEscape(String),
    #[error("symlink target escapes workspace: {0}")] SymlinkEscape(String),
    #[error("path is blocked by policy: {0}")] BlockedPath(String),
    #[error("path not in allowed write paths: {0}")] NotAllowed(String),
    #[error("command is blocked by policy: {0}")] BlockedCommand(String),
    #[error("unsafe shell syntax: {0}")] UnsafeShellSyntax(String),
    #[error("command requires approval and was denied: {0}")] CommandNotApproved(String),
    #[error("write requires approval and was denied: {0}")] WriteNotApproved(String),
}

impl AutoAgentError {
    pub fn exit_code(&self) -> i32 {
        match self {
            AutoAgentError::Config(_) | AutoAgentError::Workspace(_) => 2,
            AutoAgentError::Plan(_) => 3,
            AutoAgentError::Policy(_) => 4,
            AutoAgentError::Editing(_) => 5,
            AutoAgentError::Validation(_) => 6,
            AutoAgentError::Revert(_) => 7,
            AutoAgentError::Memory(_) => 8,
            AutoAgentError::Io(_) | AutoAgentError::Serde(_) => 1,
            AutoAgentError::Analysis(_) => 1,
        }
    }
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

pub type Result<T> = std::result::Result<T, AutoAgentError>;
```
`error/mod.rs`:
```rust
mod autoagent_error;
pub use autoagent_error::{AutoAgentError, PolicyError, Result};
```

**Step 4: Run to verify it passes**
`cargo test -p autoagent-core error` → Expected: PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(core): error taxonomy with exit-code and error-code mapping"`

---

### Task 3: Domain types (typed-first contracts)

**Files:**
- Create: `crates/autoagent-core/src/editing/file_operation.rs`
- Create: `crates/autoagent-core/src/runtime/run_state.rs`
- Create: `crates/autoagent-core/src/runtime/task_context.rs`
- Create: `crates/autoagent-core/src/planning/plan.rs`
- Create: `crates/autoagent-core/src/validation/validation_report.rs`
- Create: `mod.rs` for each of `editing/`, `runtime/`, `planning/`, `validation/`

**Step 1: Write the failing test** (`plan.rs`)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_roundtrips_json() {
        let json = r#"{"objective":"o","summary":"s","files_to_read":[],
          "files_to_create":[],"files_to_modify":[],
          "operations":[{"kind":"Create","path":"a.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"x"}],
          "validation_commands":["cargo build"],"risks":[],"rollback_strategy":"snapshot"}"#;
        let p: Plan = serde_json::from_str(json).unwrap();
        assert_eq!(p.operations.len(), 1);
        assert_eq!(p.rollback_strategy, "snapshot");
    }
}
```

**Step 2: Run to verify it fails**
`cargo test -p autoagent-core plan` → Expected: FAIL

**Step 3: Write minimal implementation**
Transcribe the structs **verbatim from SPEC-1 §3.3** into their files (`RunState`, `AgentMode`, `TaskContext`, `FileOperationKind`, `FileOperation`, `Plan`, `PlannedFile`, `ValidationReport`, `CommandValidationResult`). Each derives `Debug, Clone, Serialize, Deserialize` (+ `PartialEq, Eq` on the enums). Add `pub mod` lines to `lib.rs`:
```rust
pub mod error;
pub mod editing;
pub mod runtime;
pub mod planning;
pub mod validation;
```
Each new directory gets a `mod.rs` re-exporting its types (e.g. `editing/mod.rs`: `pub mod file_operation;`).

**Step 4: Run to verify it passes**
`cargo test -p autoagent-core plan` → Expected: PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(core): domain types (run state, file op, plan, task context, report)"`

---

### Task 4: Config schema + loader

**Files:**
- Create: `crates/autoagent-core/src/config/config_schema.rs`
- Create: `crates/autoagent-core/src/config/config_loader.rs`
- Create: `crates/autoagent-core/src/config/default_config.rs`
- Create: `crates/autoagent-core/src/config/mod.rs`

**Step 1: Write the failing test** (`config_loader.rs`)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loads_minimal_config() {
        let toml = r#"
            [project] name="autoagent" type="rust-cli" language="rust" package_manager="cargo"
            [agent] mode="supervised" allow_self_modification=false max_steps_per_run=8
            require_approval_before_write=true require_approval_before_command=true
            [workspace] root="." include=["src/**/*.rs"] exclude=["target/**"]
            [commands] test="cargo test" lint="cargo clippy" format="cargo fmt" build="cargo build"
            [safety] allowed_write_paths=["src/"] blocked_write_paths=[".git/"]
            allowed_commands=["cargo test"] blocked_commands=["sudo"]
            [memory] enabled=true directory=".agent/memory"
            [logging] directory=".agent/logs" level="info"
            [patches] directory=".agent/patches" create_before_write=true
            [runs] directory=".agent/runs"
        "#;
        let cfg = AutoAgentConfig::from_toml_str(toml).unwrap();
        assert_eq!(cfg.agent.max_steps_per_run, 8);
        assert!(cfg.safety.blocked_write_paths.contains(&".git/".to_string()));
    }
    #[test]
    fn missing_config_is_config_error() {
        let err = AutoAgentConfig::from_toml_str("not valid toml {{{").unwrap_err();
        assert_eq!(err.error_code(), "config");
    }
}
```

**Step 2: Run to verify it fails** → `cargo test -p autoagent-core config` → FAIL

**Step 3: Write minimal implementation**
`config_schema.rs` — `#[derive(Debug, Clone, Serialize, Deserialize)]` structs mirroring SPEC-1 §6 / Appendix C: `AutoAgentConfig { project, agent, workspace, commands, safety, memory, logging, patches, runs }` and each sub-struct with the exact fields from the canonical `Autoagent.toml`. Add:
```rust
impl AutoAgentConfig {
    pub fn from_toml_str(s: &str) -> crate::error::Result<Self> {
        toml::from_str(s).map_err(|e| crate::error::AutoAgentError::Config(e.to_string()))
    }
    pub fn load(root: &camino::Utf8Path) -> crate::error::Result<Self> {
        let path = root.join("Autoagent.toml");
        let text = std::fs::read_to_string(&path)
            .map_err(|_| crate::error::AutoAgentError::Config(format!("no Autoagent.toml at {path}")))?;
        Self::from_toml_str(&text)
    }
}
```
`default_config.rs` — a `pub fn default_toml() -> String` returning the canonical Appendix C config (used by `init`).

**Step 4: Run to verify it passes** → `cargo test -p autoagent-core config` → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(core): Autoagent.toml schema, loader, and default config"`

---

### Task 5: Path guard (TDD + proptest)

**Files:**
- Create: `crates/autoagent-core/src/safety/path_guard.rs`
- Create: `crates/autoagent-core/src/safety/mod.rs`

**Step 1: Write the failing tests** (`path_guard.rs`) — one per rule in SPEC-1 §3.7 precedence, plus a proptest:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn guard() -> PathGuard {
        PathGuard::new(
            Utf8PathBuf::from("/ws"),
            vec!["crates/".into(), "src/".into()],   // allowed_write
            vec![".git/".into(), "target/".into(), ".env".into()], // blocked_write
        )
    }

    #[test] fn rejects_parent_escape() {
        let e = guard().check("../secret".into(), Access::Write).unwrap_err();
        assert_eq!(e.code(), "path_escape");
    }
    #[test] fn block_overrides_allow() {
        // crates/.env is under allowed `crates/` but matches blocked `.env`
        let e = guard().check("crates/.env".into(), Access::Write).unwrap_err();
        assert_eq!(e.code(), "blocked_path");
    }
    #[test] fn allows_normal_write() {
        assert!(guard().check("crates/x/src/lib.rs".into(), Access::Write).is_ok());
    }
    #[test] fn write_outside_allowlist_rejected() {
        let e = guard().check("docs/readme.md".into(), Access::Write).unwrap_err();
        assert_eq!(e.code(), "not_allowed");
    }

    proptest::proptest! {
        // No input containing ".." or a leading "/" may ever be allowed for Write.
        #[test]
        fn fuzz_no_escape_ever_allowed(seg in "\\.\\./[a-z/]{0,20}") {
            let res = guard().check(seg.as_str().into(), Access::Write);
            prop_assert!(res.is_err());
        }
    }
}
```

**Step 2: Run to verify it fails** → `cargo test -p autoagent-core path_guard` → FAIL

**Step 3: Write minimal implementation** — implement the SPEC-1 §3.7 algorithm exactly (steps 1–7, precedence escape > symlink-escape > block > allow):
```rust
use crate::error::{PolicyError, Result, AutoAgentError};
use camino::{Utf8Path, Utf8PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)] pub enum Access { Read, Write }

pub struct PathGuard {
    root: Utf8PathBuf,
    allowed_write: Vec<String>,
    blocked_write: Vec<String>,
}

impl PathGuard {
    pub fn new(root: Utf8PathBuf, allowed_write: Vec<String>, blocked_write: Vec<String>) -> Self {
        Self { root, allowed_write, blocked_write }
    }

    pub fn check(&self, path: Utf8PathBuf, access: Access) -> Result<Utf8PathBuf> {
        // 1. empty / NUL
        if path.as_str().is_empty() || path.as_str().contains('\0') {
            return Err(PolicyError::PathEscape(path.to_string()).into());
        }
        // 2. canonicalize intent (lexical, no disk)
        let joined = if path.is_absolute() { path.clone() } else { self.root.join(&path) };
        let normalized = lexical_normalize(&joined);
        // 3. escape check
        if !normalized.starts_with(&self.root) {
            return Err(PolicyError::PathEscape(path.to_string()).into());
        }
        let rel = normalized.strip_prefix(&self.root).unwrap_or(&normalized);
        // 4. symlink resolution (best effort) on the longest existing prefix
        if let Some(real) = realpath_existing_prefix(&normalized) {
            if !real.starts_with(&self.root) {
                return Err(PolicyError::SymlinkEscape(path.to_string()).into());
            }
        }
        // 5. block list (highest precedence)
        if self.blocked_write.iter().any(|b| under(rel, b)) || builtin_denied(rel) {
            return Err(PolicyError::BlockedPath(path.to_string()).into());
        }
        // 6. allow (write only)
        if access == Access::Write && !self.allowed_write.iter().any(|a| under(rel, a)) {
            return Err(PolicyError::NotAllowed(path.to_string()).into());
        }
        Ok(normalized)
    }
}

fn lexical_normalize(p: &Utf8Path) -> Utf8PathBuf {
    let mut out: Vec<&str> = Vec::new();
    for c in p.components().map(|c| c.as_str()) {
        match c {
            "." => {}
            ".." => { out.pop(); }
            other => out.push(other),
        }
    }
    Utf8PathBuf::from(out.join("/"))
}

fn under(rel: &Utf8Path, prefix: &str) -> bool {
    let pfx = prefix.trim_end_matches('/');
    let s = rel.as_str();
    s == pfx || s.starts_with(&format!("{pfx}/")) || rel.file_name() == Some(pfx)
}

fn builtin_denied(rel: &Utf8Path) -> bool {
    let s = rel.as_str();
    s.starts_with(".git/") || s.contains("/node_modules/") || s.starts_with("node_modules/")
        || rel.file_name().map(|n| n.starts_with(".env")).unwrap_or(false)
}

fn realpath_existing_prefix(p: &Utf8Path) -> Option<Utf8PathBuf> {
    let mut cur = p.to_path_buf();
    loop {
        if cur.exists() {
            return std::fs::canonicalize(cur.as_std_path()).ok()
                .and_then(|pb| Utf8PathBuf::from_path_buf(pb).ok());
        }
        match cur.parent() { Some(par) => cur = par.to_path_buf(), None => return None }
    }
}
```
> Note: `AutoAgentError` import is used by the `?`/`.into()` conversions; keep it.

**Step 4: Run to verify it passes** → `cargo test -p autoagent-core path_guard` → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(safety): path guard with escape/symlink/block/allow precedence + proptest"`

---

### Task 6: Command guard (TDD + proptest)

**Files:**
- Create: `crates/autoagent-core/src/safety/command_guard.rs`
- Modify: `crates/autoagent-core/src/safety/mod.rs` (add `pub mod command_guard;`)

**Step 1: Write the failing tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn guard() -> CommandGuard {
        CommandGuard::new(vec!["cargo test".into(), "cargo build".into()],
                          vec!["sudo".into(), "rm -rf /".into(), "curl".into()])
    }
    #[test] fn allows_exact() { assert!(matches!(guard().check("cargo test"), Ok(Approved::Policy))); }
    #[test] fn blocks_fragment_in_chain() {
        let e = guard().check("cargo test && sudo rm x").unwrap_err();
        assert_eq!(e.code(), "blocked_command");
    }
    #[test] fn rejects_shell_metachars_on_nonexact() {
        let e = guard().check("cargo test > /etc/passwd").unwrap_err();
        assert_eq!(e.code(), "unsafe_shell_syntax");
    }
    proptest::proptest! {
        #[test]
        fn fuzz_blocked_substring_always_caught(pre in "[a-z ]{0,10}") {
            let cmd = format!("{pre}sudo x");
            prop_assert!(guard().check(&cmd).is_err());
        }
    }
}
```

**Step 2: Run to verify it fails** → `cargo test -p autoagent-core command_guard` → FAIL

**Step 3: Write minimal implementation** — SPEC-1 §3.7 command algorithm steps 1–4 (approval = steps 5–6 handled by the approval gate in Task 15):
```rust
use crate::error::{PolicyError, Result};

pub enum Approved { Policy, User }

pub struct CommandGuard { allowed: Vec<String>, blocked: Vec<String> }

const BUILTIN_BLOCK: &[&str] =
    &["sudo", "rm -rf /", "curl", "wget", "ssh", "scp", "chmod 777", "chown"];
const META: &[char] = &[';', '|', '&', '>', '<', '`'];

impl CommandGuard {
    pub fn new(allowed: Vec<String>, blocked: Vec<String>) -> Self { Self { allowed, blocked } }

    pub fn check(&self, raw: &str) -> Result<Approved> {
        let canon = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        // 2. blocked substring/fragment
        if self.blocked.iter().any(|b| canon.contains(b.as_str()))
            || BUILTIN_BLOCK.iter().any(|b| canon.contains(b)) {
            return Err(PolicyError::BlockedCommand(canon).into());
        }
        // 4. exact allow short-circuits the metachar rule
        if self.allowed.iter().any(|a| a == &canon) {
            return Ok(Approved::Policy);
        }
        // 3. metachars only safe inside an exact allow entry (already returned above)
        if canon.contains(META) || canon.contains("$(") {
            return Err(PolicyError::UnsafeShellSyntax(canon).into());
        }
        // 5. unknown but syntactically clean → caller routes to approval gate
        Err(PolicyError::CommandNotApproved(canon).into())
    }
}
```

**Step 4: Run to verify it passes** → `cargo test -p autoagent-core command_guard` → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(safety): command guard with fragment + metachar rejection + proptest"`

---

### Task 7: Policy engine (compose guards from config)

**Files:**
- Create: `crates/autoagent-core/src/safety/policy_engine.rs`
- Modify: `crates/autoagent-core/src/safety/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::config_schema::AutoAgentConfig;
    #[test]
    fn engine_built_from_config_blocks_git() {
        let cfg = AutoAgentConfig::from_toml_str(crate::config::default_config::default_toml().as_str()).unwrap();
        let eng = PolicyEngine::from_config(&cfg, "/ws".into());
        assert!(eng.check_write(".git/config".into()).is_err());
        assert!(eng.check_command("cargo test").is_ok());
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation**
```rust
use crate::config::config_schema::AutoAgentConfig;
use crate::error::Result;
use crate::safety::{path_guard::{PathGuard, Access}, command_guard::{CommandGuard, Approved}};
use camino::Utf8PathBuf;

pub struct PolicyEngine { paths: PathGuard, commands: CommandGuard }

impl PolicyEngine {
    pub fn from_config(cfg: &AutoAgentConfig, root: Utf8PathBuf) -> Self {
        Self {
            paths: PathGuard::new(root, cfg.safety.allowed_write_paths.clone(),
                                  cfg.safety.blocked_write_paths.clone()),
            commands: CommandGuard::new(cfg.safety.allowed_commands.clone(),
                                        cfg.safety.blocked_commands.clone()),
        }
    }
    pub fn check_write(&self, p: Utf8PathBuf) -> Result<Utf8PathBuf> { self.paths.check(p, Access::Write) }
    pub fn check_read(&self, p: Utf8PathBuf) -> Result<Utf8PathBuf> { self.paths.check(p, Access::Read) }
    pub fn check_command(&self, c: &str) -> Result<Approved> { self.commands.check(c) }
}
```

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(safety): policy engine composing path+command guards from config"`

---

### Task 8: File scanner

**Files:**
- Create: `crates/autoagent-core/src/analysis/file_scanner.rs`
- Create: `crates/autoagent-core/src/analysis/mod.rs`

**Step 1: Write the failing test** (uses `tempfile`)
```rust
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
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** using `ignore::WalkBuilder` + `globset`:
```rust
use crate::error::{AutoAgentError, Result};
use camino::{Utf8Path, Utf8PathBuf};
use globset::{Glob, GlobSetBuilder};

pub fn scan(root: &Utf8Path, include: &[String], exclude: &[String]) -> Result<Vec<Utf8PathBuf>> {
    let inc = build_set(include)?;
    let exc = build_set(exclude)?;
    let mut out = Vec::new();
    for entry in ignore::WalkBuilder::new(root).hidden(false).git_ignore(true).build() {
        let entry = entry.map_err(|e| AutoAgentError::Analysis(e.to_string()))?;
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
        let abs = Utf8PathBuf::from_path_buf(entry.into_path())
            .map_err(|_| AutoAgentError::Analysis("non-utf8 path".into()))?;
        let rel = abs.strip_prefix(root).unwrap_or(&abs);
        if exc.is_match(rel.as_str()) { continue; }
        if inc.is_match(rel.as_str()) { out.push(rel.to_path_buf()); }
    }
    out.sort();
    Ok(out)
}

fn build_set(globs: &[String]) -> Result<globset::GlobSet> {
    let mut b = GlobSetBuilder::new();
    for g in globs {
        b.add(Glob::new(g).map_err(|e| AutoAgentError::Analysis(e.to_string()))?);
    }
    b.build().map_err(|e| AutoAgentError::Analysis(e.to_string()))
}
```

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(analysis): file scanner honoring include/exclude + ignore rules"`

---

### Task 9: Snapshot manager (sha256, before/after)

**Files:**
- Create: `crates/autoagent-core/src/editing/snapshot_manager.rs`
- Modify: `crates/autoagent-core/src/editing/mod.rs`

**Step 1: Write the failing test**
```rust
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
        assert!(run.join("before/a.txt").exists());
    }
    #[test]
    fn snapshot_of_missing_file_returns_none_hash() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let mgr = SnapshotManager::new(root.join("run"));
        assert!(mgr.snapshot(root, "nope.txt".into()).is_err()); // create-target handled by editor, not snapshot
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation**
```rust
use crate::error::{AutoAgentError, Result};
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

pub struct SnapshotManager { run_dir: Utf8PathBuf }

impl SnapshotManager {
    pub fn new(run_dir: Utf8PathBuf) -> Self { Self { run_dir } }

    /// Copy <root>/<rel> into <run_dir>/before/<rel>, return its sha256.
    pub fn snapshot(&self, root: &Utf8Path, rel: Utf8PathBuf) -> Result<String> {
        let src = root.join(&rel);
        let bytes = std::fs::read(src.as_std_path())?;
        let dst = self.run_dir.join("before").join(&rel);
        if let Some(parent) = dst.parent() { std::fs::create_dir_all(parent.as_std_path())?; }
        std::fs::write(dst.as_std_path(), &bytes)?;
        Ok(sha256_hex(&bytes))
    }

    pub fn record_after(&self, root: &Utf8Path, rel: Utf8PathBuf) -> Result<String> {
        let src = root.join(&rel);
        let bytes = std::fs::read(src.as_std_path())?;
        let dst = self.run_dir.join("after").join(&rel);
        if let Some(parent) = dst.parent() { std::fs::create_dir_all(parent.as_std_path())?; }
        std::fs::write(dst.as_std_path(), &bytes)?;
        Ok(sha256_hex(&bytes))
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
```
> `AutoAgentError` import is used by the `?` on `std::io::Error` (the `#[from]` conversion).

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(editing): snapshot manager with sha256 hashing"`

---

### Task 10: Plan reader + validator (contract enforcement)

**Files:**
- Create: `crates/autoagent-core/src/planning/plan_reader.rs`
- Create: `crates/autoagent-core/src/planning/plan_validator.rs`
- Modify: `crates/autoagent-core/src/planning/mod.rs`

**Step 1: Write the failing tests** — enforce SPEC-1 §3.4.1 rules:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::policy_engine::PolicyEngine;
    use crate::config::config_schema::AutoAgentConfig;

    fn engine() -> PolicyEngine {
        let cfg = AutoAgentConfig::from_toml_str(crate::config::default_config::default_toml().as_str()).unwrap();
        PolicyEngine::from_config(&cfg, "/ws".into())
    }
    #[test] fn rejects_empty_operations() {
        let p = minimal_plan(vec![]);
        let e = validate_plan(&p, &engine()).unwrap_err();
        assert_eq!(e.error_code(), "plan");
    }
    #[test] fn rejects_blocked_write_path() {
        let op = op("Write", ".git/config", Some("x"));
        let e = validate_plan(&minimal_plan(vec![op]), &engine()).unwrap_err();
        assert_eq!(e.error_code(), "policy.blocked_path"); // policy bubbles up
    }
    #[test] fn rejects_non_snapshot_rollback() {
        let mut p = minimal_plan(vec![op("Create","crates/x.rs",Some("x"))]);
        p.rollback_strategy = "manual".into();
        assert!(validate_plan(&p, &engine()).is_err());
    }
}
```
(Add `minimal_plan` / `op` helpers in the test module.)

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation**
`plan_reader.rs`:
```rust
use crate::error::{AutoAgentError, Result};
use crate::planning::plan::Plan;
use camino::Utf8Path;

pub fn read_plan(path: &Utf8Path) -> Result<Plan> {
    let text = std::fs::read_to_string(path.as_std_path())
        .map_err(|e| AutoAgentError::Plan(format!("cannot read {path}: {e}")))?;
    serde_json::from_str(&text).map_err(|e| AutoAgentError::Plan(e.to_string()))
}
```
`plan_validator.rs` — implement every rule from the §3.4.1 tables (objective non-empty, ≥1 op, per-kind required fields, each path through `engine.check_write`, validation_commands through `engine.check_command`, rollback == "snapshot"):
```rust
use crate::error::{AutoAgentError, Result};
use crate::planning::plan::Plan;
use crate::editing::file_operation::{FileOperation, FileOperationKind};
use crate::safety::policy_engine::PolicyEngine;

pub fn validate_plan(plan: &Plan, engine: &PolicyEngine) -> Result<()> {
    if plan.objective.trim().is_empty() {
        return Err(AutoAgentError::Plan("objective is empty".into()));
    }
    if plan.operations.is_empty() {
        return Err(AutoAgentError::Plan("plan has no operations".into()));
    }
    if plan.rollback_strategy != "snapshot" {
        return Err(AutoAgentError::Plan(
            format!("unsupported rollback_strategy '{}' (only 'snapshot' in 0.1.0)", plan.rollback_strategy)));
    }
    for (i, op) in plan.operations.iter().enumerate() {
        validate_op(i, op, engine)?;
    }
    for cmd in &plan.validation_commands {
        // BlockedCommand/UnsafeShell bubble up as policy errors; CommandNotApproved is acceptable at plan time
        match engine.check_command(cmd) {
            Ok(_) => {}
            Err(e) if e.error_code() == "policy.command_not_approved" => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn validate_op(i: usize, op: &FileOperation, engine: &PolicyEngine) -> Result<()> {
    use FileOperationKind::*;
    let needs_content = matches!(op.kind, Create | Write | Replace | Append);
    if needs_content && op.content.is_none() {
        return Err(AutoAgentError::Plan(format!("op[{i}] {:?} requires content", op.kind)));
    }
    if matches!(op.kind, Rename) && op.destination_path.is_none() {
        return Err(AutoAgentError::Plan(format!("op[{i}] Rename requires destination_path")));
    }
    engine.check_write(op.path.clone())?;          // policy error bubbles up unchanged
    if let Some(dest) = &op.destination_path {
        engine.check_write(dest.clone())?;
    }
    Ok(())
}
```

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(planning): JSON plan reader and schema/policy validator"`

---

### Task 11: Event log (events.jsonl + seq + catalog)

**Files:**
- Create: `crates/autoagent-core/src/logging/event_log.rs`
- Create: `crates/autoagent-core/src/logging/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appends_monotonic_seq() {
        let dir = tempfile::tempdir().unwrap();
        let path = camino::Utf8Path::from_path(dir.path()).unwrap().join("events.jsonl");
        let mut log = EventLog::new(path.clone(), "run-1".into());
        log.emit("run_started", "Created", serde_json::json!({"objective":"o"})).unwrap();
        log.emit("run_completed", "Completed", serde_json::json!({"state":"Completed"})).unwrap();
        let lines: Vec<_> = std::fs::read_to_string(path.as_std_path()).unwrap().lines().map(|l| l.to_string()).collect();
        assert_eq!(lines.len(), 2);
        let first: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(first["seq"], 1);
        assert_eq!(first["type"], "run_started");
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation**
```rust
use crate::error::Result;
use camino::Utf8PathBuf;
use std::io::Write;

pub struct EventLog { path: Utf8PathBuf, run_id: String, seq: u64 }

impl EventLog {
    pub fn new(path: Utf8PathBuf, run_id: String) -> Self { Self { path, run_id, seq: 0 } }

    pub fn emit(&mut self, ty: &str, state: &str, data: serde_json::Value) -> Result<()> {
        self.seq += 1;
        if let Some(p) = self.path.parent() { std::fs::create_dir_all(p.as_std_path())?; }
        let evt = serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "run_id": self.run_id, "seq": self.seq,
            "type": ty, "state": state, "data": data,
        });
        let mut f = std::fs::OpenOptions::new().create(true).append(true)
            .open(self.path.as_std_path())?;
        writeln!(f, "{evt}")?;
        Ok(())
    }
}
```
> The event `type` strings used across the engine MUST come from SPEC-1 §3.4.3 catalog (`run_started`, `plan_loaded`, `plan_rejected`, `snapshot_created`, `operation_applied`, `command_started/finished`, `validation_completed`, `run_completed/failed`, `revert_*`, `drift_detected`). Keep a `pub const` list to prevent typos.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(logging): append-only events.jsonl with monotonic seq"`

---

### Task 12: File editor + patch writer

**Files:**
- Create: `crates/autoagent-core/src/editing/file_editor.rs`
- Create: `crates/autoagent-core/src/editing/patch_writer.rs`
- Create: `crates/autoagent-core/src/editing/diff_builder.rs`
- Modify: `crates/autoagent-core/src/editing/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::file_operation::{FileOperation, FileOperationKind};
    #[test]
    fn applies_create_then_replace() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let ed = FileEditor::new(root.to_path_buf());
        ed.apply(&FileOperation{kind:FileOperationKind::Create, path:"a.txt".into(),
            destination_path:None, reason:"r".into(), before_hash:None, after_hash:None,
            content:Some("v1".into())}).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v1");
        ed.apply(&FileOperation{kind:FileOperationKind::Replace, path:"a.txt".into(),
            destination_path:None, reason:"r".into(), before_hash:None, after_hash:None,
            content:Some("v2".into())}).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2");
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation**
```rust
use crate::error::{AutoAgentError, Result};
use crate::editing::file_operation::{FileOperation, FileOperationKind};
use camino::Utf8PathBuf;

pub struct FileEditor { root: Utf8PathBuf }

impl FileEditor {
    pub fn new(root: Utf8PathBuf) -> Self { Self { root } }

    pub fn apply(&self, op: &FileOperation) -> Result<()> {
        use FileOperationKind::*;
        let abs = self.root.join(&op.path);
        if let Some(parent) = abs.parent() { std::fs::create_dir_all(parent.as_std_path())?; }
        match op.kind {
            Create | Write | Replace => {
                let c = op.content.as_ref().ok_or_else(|| AutoAgentError::Editing("missing content".into()))?;
                std::fs::write(abs.as_std_path(), c)?;
            }
            Append => {
                use std::io::Write;
                let c = op.content.as_ref().ok_or_else(|| AutoAgentError::Editing("missing content".into()))?;
                let mut f = std::fs::OpenOptions::new().create(true).append(true).open(abs.as_std_path())?;
                f.write_all(c.as_bytes())?;
            }
            Delete => { std::fs::remove_file(abs.as_std_path())?; }
            Rename => {
                let dest = op.destination_path.as_ref()
                    .ok_or_else(|| AutoAgentError::Editing("rename missing destination".into()))?;
                std::fs::rename(abs.as_std_path(), self.root.join(dest).as_std_path())?;
            }
            CreateDirectory => { std::fs::create_dir_all(abs.as_std_path())?; }
        }
        Ok(())
    }
}
```
`diff_builder.rs` — `pub fn unified(before: &str, after: &str, path: &str) -> String` using `similar::TextDiff::from_lines(...).unified_diff()`.
`patch_writer.rs` — `pub fn write_patch(run_id, ops_with_before_after) -> Result<Utf8PathBuf>` concatenating per-file unified diffs into `.agent/patches/<run-id>.patch`.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(editing): file editor, unified diff builder, patch writer"`

---

### Task 13: Run logger (run.json + run folder)

**Files:**
- Create: `crates/autoagent-core/src/logging/run_logger.rs`
- Modify: `crates/autoagent-core/src/logging/mod.rs`

**Step 1: Write the failing test** — assert the `run.json` shape from SPEC-1 §3.4.2:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writes_run_json_with_required_fields() {
        let dir = tempfile::tempdir().unwrap();
        let run_dir = camino::Utf8Path::from_path(dir.path()).unwrap().to_path_buf();
        let mut rl = RunLogger::create(run_dir.clone(), "20260608T000000Z-x".into(), "obj".into());
        rl.set_state("Completed");
        rl.record_file("a.txt", "Replace", "h1", "h2");
        rl.finish(true).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(run_dir.join("run.json")).unwrap()).unwrap();
        assert_eq!(v["state"], "Completed");
        assert_eq!(v["validation_passed"], true);
        assert_eq!(v["files_modified"][0]["path"], "a.txt");
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — a `RunLogger` that owns the run-folder layout (creates `before/`, `after/`, writes `objective.md`, accumulates `files_modified`/`commands_executed`, and serializes `run.json` per §3.4.2). Fields: `run_id, task_id, objective, mode, self_modification, state, started_at, ended_at, duration_ms, plan_path, files_read, files_modified[], commands_executed[], validation_passed, patch_path, approvals[], reverted_at`.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(logging): run logger emitting run.json + run folder layout"`

---

### Task 14: Agent runtime — apply loop

**Files:**
- Create: `crates/autoagent-core/src/runtime/agent_loop.rs`
- Create: `crates/autoagent-core/src/runtime/agent_runtime.rs`
- Create: `crates/autoagent-core/src/runtime/mod.rs`

**Step 1: Write the failing E2E-ish core test** (no CLI, real temp workspace):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn apply_plan_creates_file_and_reversible_run() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("Autoagent.toml"),
            crate::config::default_config::default_toml()).unwrap();
        // plan: create crates/demo.rs
        let plan = root.join("p.plan.json");
        std::fs::write(&plan, r#"{"objective":"demo","summary":"s","files_to_read":[],
          "files_to_create":[{"path":"crates/demo.rs","purpose":"x"}],"files_to_modify":[],
          "operations":[{"kind":"Create","path":"crates/demo.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"// demo"}],
          "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#).unwrap();
        let run_id = apply(root, &plan, /*auto_approve=*/true).unwrap();
        assert!(root.join("crates/demo.rs").exists());
        assert!(root.join(format!(".agent/runs/{run_id}/run.json")).exists());
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `agent_loop::apply(root, plan_path, auto_approve)` executing SPEC-1 §3.5 steps for the apply path: load config → build PolicyEngine → read plan → validate_plan → (approval unless auto) → for each op: snapshot existing target, emit `snapshot_created`, apply via FileEditor, record after-hash, emit `operation_applied` → write patch → write run.json (`Completed`). Each privileged step returns `Result`; on any error, set state `Failed`, persist the run folder (snapshots already present → reversible), and propagate. Generate `run_id` = `<UTC compact>-<slug(objective)>` and `task_id` = `Uuid::new_v4()`.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(runtime): apply loop (validate→snapshot→apply→patch→run.json)"`

---

### Task 15: Approval gate

**Files:**
- Create: `crates/autoagent-core/src/safety/approval_gate.rs`
- Modify: `crates/autoagent-core/src/safety/mod.rs`
- Modify: `crates/autoagent-core/src/runtime/agent_loop.rs` (wire the gate before writes/commands)

**Step 1: Write the failing test** — a trait so the CLI injects the real prompt and tests inject auto-yes/no:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn auto_deny_blocks_write() {
        let gate = AutoGate::deny();
        assert!(gate.confirm_write("crates/x.rs").is_err());
    }
    #[test] fn auto_allow_passes() {
        assert!(AutoGate::allow().confirm_write("crates/x.rs").is_ok());
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation**
```rust
use crate::error::{PolicyError, Result};

pub trait ApprovalGate {
    fn confirm_write(&self, target: &str) -> Result<()>;
    fn confirm_command(&self, command: &str) -> Result<()>;
}

/// Non-interactive gate for tests and `--yes`.
pub struct AutoGate { yes: bool }
impl AutoGate {
    pub fn allow() -> Self { Self { yes: true } }
    pub fn deny() -> Self { Self { yes: false } }
}
impl ApprovalGate for AutoGate {
    fn confirm_write(&self, t: &str) -> Result<()> {
        if self.yes { Ok(()) } else { Err(PolicyError::WriteNotApproved(t.into()).into()) }
    }
    fn confirm_command(&self, c: &str) -> Result<()> {
        if self.yes { Ok(()) } else { Err(PolicyError::CommandNotApproved(c.into()).into()) }
    }
}
```
The interactive `DialoguerGate` lives in `autoagent-cli` (Task 18) and implements the same trait using `dialoguer::Confirm`. Wire `agent_loop::apply` to take `&dyn ApprovalGate` and call it before the first write when `require_approval_before_write` is true.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(safety): approval gate trait + non-interactive AutoGate"`

---

### Task 16: Revert

**Files:**
- Create: `crates/autoagent-core/src/runtime/revert.rs`
- Modify: `crates/autoagent-core/src/runtime/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revert_restores_modified_file() {
        // arrange via apply (Task 14), mutate a tracked file, then revert
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("Autoagent.toml"), crate::config::default_config::default_toml()).unwrap();
        std::fs::create_dir_all(root.join("crates")).unwrap();
        std::fs::write(root.join("crates/a.rs"), "ORIGINAL").unwrap();
        let plan = root.join("p.json");
        std::fs::write(&plan, r#"{"objective":"edit","summary":"s","files_to_read":[],
          "files_to_create":[],"files_to_modify":[{"path":"crates/a.rs","purpose":"x"}],
          "operations":[{"kind":"Replace","path":"crates/a.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"CHANGED"}],
          "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#).unwrap();
        let run_id = crate::runtime::agent_loop::apply(root, &plan, true).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("crates/a.rs")).unwrap(), "CHANGED");
        revert(root, &run_id).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("crates/a.rs")).unwrap(), "ORIGINAL");
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `revert(root, run_id)`: read `run.json`; for each `files_modified` entry, read the `before/<rel>` snapshot, verify the current file's sha256 equals the recorded `after_hash` (else emit `drift_detected` and refuse to overwrite that file), restore the snapshot, emit `revert_*` events, then set `run.json.state="Reverted"` and `reverted_at`. Created files (no `before/` snapshot) are deleted on revert; deleted files are restored from `before/`.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(runtime): revert with drift detection from before/ snapshots"`

---

### Task 17: `init` + `doctor` core

**Files:**
- Create: `crates/autoagent-core/src/runtime/init.rs`
- Create: `crates/autoagent-core/src/runtime/doctor.rs`

**Step 1: Write the failing tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn init_writes_config_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        init_workspace(root).unwrap();
        assert!(root.join("Autoagent.toml").exists());
        assert!(root.join(".agent/runs").exists());
    }
    #[test] fn doctor_reports_config_presence() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        init_workspace(root).unwrap();
        let report = doctor(root);
        assert!(report.checks.iter().any(|c| c.name=="config" && c.ok));
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `init_workspace(root)` writes `default_toml()` to `Autoagent.toml` and creates `.agent/{memory,plans,runs,patches,logs,reports,tools}`. `doctor(root) -> DoctorReport { checks: Vec<Check{name,ok,detail}> }` probing: rust/cargo on PATH (`which`-style via `Command::new("cargo").arg("--version")`), git presence, config parse, `.agent` writability, each `[commands]` binary resolvable. Read-only — no writes.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(runtime): init workspace scaffolding and doctor health checks"`

---

### Task 18: CLI wiring (clap) + interactive gate

**Files:**
- Modify: `crates/autoagent-cli/src/main.rs`
- Create: `crates/autoagent-cli/src/commands/mod.rs` and one file per command (`init.rs`, `doctor.rs`, `apply.rs`, `revert.rs`, `patch.rs`, `config.rs`)
- Create: `crates/autoagent-cli/src/approval.rs` (DialoguerGate)

**Step 1: Write the failing test** (CLI parse test in `main.rs`)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[test] fn parses_apply_with_path() {
        let cli = Cli::try_parse_from(["autoagent","apply","p.plan.json"]).unwrap();
        assert!(matches!(cli.command, Command::Apply{..}));
    }
    #[test] fn parses_yes_flag() {
        let cli = Cli::try_parse_from(["autoagent","--yes","init"]).unwrap();
        assert!(cli.yes);
    }
}
```

**Step 2: Run to verify it fails** → `cargo test -p autoagent-cli` → FAIL

**Step 3: Write minimal implementation** — `#[derive(Parser)] Cli { #[arg(long)] yes: bool, #[command(subcommand)] command: Command }` with the 0.1.0 subcommands (`Init`, `Doctor`, `Apply{plan}`, `Revert{run_id}`, `Patch{List|Show{run_id}}`, `Config{Show}`). `main()` matches, builds the `DialoguerGate` (or `AutoGate::allow()` when `--yes`), calls into `autoagent-core`, and maps `AutoAgentError::exit_code()` to `std::process::exit`. `approval.rs` implements `ApprovalGate` via `dialoguer::Confirm`.

**Step 4: Run to verify it passes** → `cargo test -p autoagent-cli` → PASS

**Step 5: Commit**
`git add -A && git commit -m "feat(cli): clap command surface, exit-code mapping, interactive approval gate"`

---

### Task 19: End-to-end test (real binary, real workspace)

**Files:**
- Create: `crates/autoagent-cli/tests/e2e_apply_revert.rs`

This is a **genuine E2E test** per the project's definition: it invokes the compiled `autoagent` binary as a subprocess against a real throwaway workspace on the real filesystem — no mocked layers. (It does not call an LLM; M1 has none, so nothing is stubbed.)

**Step 1: Write the failing test**
```rust
use std::process::Command;

fn bin() -> &'static str { env!("CARGO_BIN_EXE_autoagent") }

#[test]
fn init_apply_revert_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // init
    let out = Command::new(bin()).args(["--yes","init"]).current_dir(root).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(root.join("Autoagent.toml").exists());

    // seed a tracked file + plan that edits it
    std::fs::create_dir_all(root.join("crates")).unwrap();
    std::fs::write(root.join("crates/a.rs"), "ORIGINAL").unwrap();
    std::fs::write(root.join("p.plan.json"), r#"{"objective":"edit a","summary":"s",
      "files_to_read":[],"files_to_create":[],"files_to_modify":[{"path":"crates/a.rs","purpose":"x"}],
      "operations":[{"kind":"Replace","path":"crates/a.rs","destination_path":null,"reason":"r",
        "before_hash":null,"after_hash":null,"content":"CHANGED"}],
      "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#).unwrap();

    // apply
    let out = Command::new(bin()).args(["--yes","apply","p.plan.json"]).current_dir(root).output().unwrap();
    assert!(out.status.success());
    assert_eq!(std::fs::read_to_string(root.join("crates/a.rs")).unwrap(), "CHANGED");

    // discover run id and revert
    let runs_dir = root.join(".agent/runs");
    let run_id = std::fs::read_dir(&runs_dir).unwrap().next().unwrap().unwrap().file_name();
    let out = Command::new(bin()).args(["--yes","revert", run_id.to_str().unwrap()])
        .current_dir(root).output().unwrap();
    assert!(out.status.success());
    assert_eq!(std::fs::read_to_string(root.join("crates/a.rs")).unwrap(), "ORIGINAL");
}
```

**Step 2: Run to verify it fails** (before wiring complete) → FAIL

**Step 3: Make it pass** — fix any wiring gaps surfaced (cwd handling, run-id discovery, exit codes). No production code shortcuts.

**Step 4: Run to verify it passes** → `cargo test -p autoagent-cli --test e2e_apply_revert` → PASS

**Step 5: Commit**
`git add -A && git commit -m "test(e2e): real-binary init→apply→revert roundtrip"`

---

### Task 20: Quality gate + milestone exit

**Step 1: Full gate (the same commands the tool guards)**
```
cargo fmt --all -- --check    → Expected: clean
cargo clippy --all-targets --all-features -- -D warnings   → Expected: zero warnings
cargo test --workspace        → Expected: all green
cargo build --release         → Expected: builds autoagent binary
```

**Step 2: Verify M1 exit criteria (SPEC-1 §5)**
- `init`, `doctor`, `apply <plan.json>`, `revert <run-id>` work E2E ✓ (Task 19)
- Every applied run snapshotted + reversible ✓ (Tasks 9, 14, 16)
- Zero out-of-policy writes in the suite ✓ (Tasks 5–7, 10)

**Step 3: Commit**
`git add -A && git commit -m "chore(0.1.0): mutation engine milestone exit — gate green"`

---

## Notes / deferred (out of scope for M1)
- LLM planning, `analyze`, `run`, `evolve`, `memory`, plugins → M2–M8.
- `git` read-only client (`git status/diff`) is only needed from M6; not built here.
- Workspace-level `.agent/logs/events.jsonl` aggregation across runs is wired in Task 11's emitter but cross-run querying tooling is deferred.
