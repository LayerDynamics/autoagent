# M2 — 0.2.0 Project Analyzer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `lore:execute` to implement this plan task-by-task.
> **Scope guard:** Do ONLY what is listed here. If you discover adjacent issues, note them as a TODO and continue. Do NOT fix them.

**Goal:** Add `analyze` — detect language, package manager, dependencies, and a source-tree summary, and write `reports/project-analysis.md`.
**Architecture:** Read-only analysis pipeline in `autoagent-core/src/analysis/` built on M1's file scanner and path guard. Produces a typed `ProjectAnalysis` and renders it to Markdown.
**Tech Stack:** Rust 2021; reuses M1 deps (serde, toml, camino, ignore). No new runtime deps.
**Practices:** TDD, typed-interfaces-first, contract-first.
**Required skills:** none.
**Prerequisite:** **M1 complete** (uses `file_scanner::scan`, `PathGuard`, `AutoAgentConfig`, `AutoAgentError`).
**Design status:** ⚠️ **PROPOSED DESIGN.** SPEC-1 §13 names the deliverables ("language detection, Cargo/package.json detection, dependency summaries, source tree summaries, project report writer") but specifies no types or algorithms. The `ProjectAnalysis` schema and detection heuristics below are design decisions to confirm during execution, not extracted facts.

**Contracts introduced here (new):** `ProjectAnalysis`, `LanguageKind`, `PackageManager`, `DependencySummary`. These are M2-owned; later milestones may read them.

---

### Task 1: `ProjectAnalysis` types (typed-first, contract-first)

**Files:**
- Create: `crates/autoagent-core/src/analysis/project_analysis.rs`
- Modify: `crates/autoagent-core/src/analysis/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn analysis_serializes() {
        let a = ProjectAnalysis {
            root: "/ws".into(), language: LanguageKind::Rust,
            package_manager: Some(PackageManager::Cargo),
            dependencies: vec![DependencySummary{ name:"serde".into(), version:"1".into(), dev:false }],
            file_count: 12, source_files: 8,
            top_dirs: vec!["crates".into(), "docs".into()],
        };
        let j = serde_json::to_string(&a).unwrap();
        assert!(j.contains("\"language\":\"Rust\""));
    }
}
```

**Step 2: Run to verify it fails** → `cargo test -p autoagent-core project_analysis` → FAIL

**Step 3: Write minimal implementation**
```rust
use camino::Utf8PathBuf;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LanguageKind { Rust, JavaScript, TypeScript, Mixed, Unknown }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PackageManager { Cargo, Npm, Pnpm, Yarn }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySummary { pub name: String, pub version: String, pub dev: bool }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAnalysis {
    pub root: Utf8PathBuf,
    pub language: LanguageKind,
    pub package_manager: Option<PackageManager>,
    pub dependencies: Vec<DependencySummary>,
    pub file_count: usize,
    pub source_files: usize,
    pub top_dirs: Vec<String>,
}
```

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(analysis): ProjectAnalysis types"`

---

### Task 2: Language + package-manager detection

**Files:**
- Create: `crates/autoagent-core/src/analysis/project_analyzer.rs`
- Modify: `crates/autoagent-core/src/analysis/mod.rs`

**Step 1: Write the failing tests** (PROPOSED heuristic — confirm thresholds):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn detects_rust_by_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"x\"\nversion=\"0.1.0\"").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "fn main(){}").unwrap();
        let (lang, pm) = detect(root).unwrap();
        assert_eq!(lang, LanguageKind::Rust);
        assert_eq!(pm, Some(PackageManager::Cargo));
    }
    #[test] fn detects_npm_by_package_json_and_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("package.json"), r#"{"name":"x","dependencies":{}}"#).unwrap();
        std::fs::write(root.join("package-lock.json"), "{}").unwrap();
        let (_, pm) = detect(root).unwrap();
        assert_eq!(pm, Some(PackageManager::Npm));
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — detection rules (PROPOSED):
- `Cargo.toml` present → `LanguageKind::Rust`, `PackageManager::Cargo`.
- `package.json` present → JS/TS (TS if any `tsconfig.json` or `.ts` files dominate `.js`); package manager from lockfile precedence: `pnpm-lock.yaml`→Pnpm, `yarn.lock`→Yarn, `package-lock.json`→Npm, else Npm.
- Both Cargo + package.json → `Mixed`.
- Neither → `Unknown`, `None`.
```rust
use crate::error::Result;
use crate::analysis::project_analysis::{LanguageKind, PackageManager};
use camino::Utf8Path;

pub fn detect(root: &Utf8Path) -> Result<(LanguageKind, Option<PackageManager>)> {
    let has_cargo = root.join("Cargo.toml").exists();
    let has_pkg = root.join("package.json").exists();
    let lang = match (has_cargo, has_pkg) {
        (true, true) => LanguageKind::Mixed,
        (true, false) => LanguageKind::Rust,
        (false, true) => if root.join("tsconfig.json").exists() { LanguageKind::TypeScript }
                         else { LanguageKind::JavaScript },
        (false, false) => LanguageKind::Unknown,
    };
    let pm = if has_cargo { Some(PackageManager::Cargo) }
        else if has_pkg {
            if root.join("pnpm-lock.yaml").exists() { Some(PackageManager::Pnpm) }
            else if root.join("yarn.lock").exists() { Some(PackageManager::Yarn) }
            else { Some(PackageManager::Npm) }
        } else { None };
    Ok((lang, pm))
}
```

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(analysis): language + package-manager detection"`

---

### Task 3: Dependency summary (Cargo + package.json parsers)

**Files:**
- Create: `crates/autoagent-core/src/analysis/dependency_analyzer.rs`
- Modify: `crates/autoagent-core/src/analysis/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parses_cargo_deps() {
        let toml = r#"[package]
name="x"
version="0.1.0"
[dependencies]
serde="1"
[dev-dependencies]
proptest="1""#;
        let deps = parse_cargo(toml).unwrap();
        assert!(deps.iter().any(|d| d.name=="serde" && !d.dev));
        assert!(deps.iter().any(|d| d.name=="proptest" && d.dev));
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `parse_cargo(&str) -> Result<Vec<DependencySummary>>` via `toml::Value`, walking `[dependencies]` (dev=false) and `[dev-dependencies]` (dev=true), normalizing both `serde = "1"` and `serde = { version = "1" }` forms. `parse_package_json(&str)` via `serde_json::Value`, walking `dependencies` and `devDependencies`. Version unknown → `"*"`.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(analysis): Cargo + package.json dependency parsing"`

---

### Task 4: Source-tree summary + assemble `ProjectAnalysis`

**Files:**
- Create: `crates/autoagent-core/src/analysis/source_map_builder.rs`
- Modify: `crates/autoagent-core/src/analysis/project_analyzer.rs` (add `analyze(root, &config) -> ProjectAnalysis`)

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::config_schema::AutoAgentConfig;
    #[test] fn analyze_produces_counts() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"x\"\nversion=\"0.1.0\"\n[dependencies]\nserde=\"1\"").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "fn a(){}").unwrap();
        let cfg = AutoAgentConfig::from_toml_str(crate::config::default_config::default_toml().as_str()).unwrap();
        let a = analyze(root, &cfg).unwrap();
        assert_eq!(a.language, crate::analysis::project_analysis::LanguageKind::Rust);
        assert!(a.source_files >= 1);
        assert!(a.dependencies.iter().any(|d| d.name=="serde"));
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `analyze`: call `detect`, run `file_scanner::scan` with config include/exclude for `file_count`+`source_files` (source = `.rs`/`.ts`/`.js`), parse deps from the detected manifest, compute `top_dirs` (first-level dirs under root, sorted, excluding `.agent`/`target`/`.git`). Assemble `ProjectAnalysis`.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(analysis): source-tree summary + analyze() assembler"`

---

### Task 5: Markdown report writer

**Files:**
- Create: `crates/autoagent-core/src/analysis/report_writer.rs`
- Modify: `crates/autoagent-core/src/analysis/mod.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn renders_and_writes_report() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let a = sample_analysis(root); // helper building a ProjectAnalysis
        let path = write_report(root, &a).unwrap();
        let md = std::fs::read_to_string(path.as_std_path()).unwrap();
        assert!(md.starts_with("# Project Analysis"));
        assert!(md.contains("## Dependencies"));
        assert!(root.join(".agent/reports/project-analysis.md").exists());
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `render(&ProjectAnalysis) -> String` producing `# Project Analysis`, a summary table (language, package manager, file/source counts), a `## Dependencies` table, and a `## Top-level layout` list. `write_report(root, &a)` writes it to `.agent/reports/project-analysis.md` (creating the dir) and returns the path. Every code block / table correctly fenced.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(analysis): Markdown project-analysis report writer"`

---

### Task 6: `analyze` CLI command + E2E

**Files:**
- Create: `crates/autoagent-cli/src/commands/analyze.rs`
- Modify: `crates/autoagent-cli/src/main.rs` (add `Analyze` subcommand)
- Create: `crates/autoagent-cli/tests/e2e_analyze.rs`

**Step 1: Write the failing E2E** (real binary, real repo):
```rust
use std::process::Command;
fn bin() -> &'static str { env!("CARGO_BIN_EXE_autoagent") }
#[test] fn analyze_writes_report() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Command::new(bin()).args(["--yes","init"]).current_dir(root).output().unwrap();
    std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"x\"\nversion=\"0.1.0\"\n[dependencies]\nserde=\"1\"").unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "fn a(){}").unwrap();
    let out = Command::new(bin()).args(["analyze"]).current_dir(root).output().unwrap();
    assert!(out.status.success());
    assert!(root.join(".agent/reports/project-analysis.md").exists());
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `Analyze` subcommand loads config, calls `analyze`, `write_report`, prints the report path. Read-only (writes only the report, matching SPEC-1 §3.4 default write behavior).

**Step 4: Run to verify it passes** → `cargo test -p autoagent-cli --test e2e_analyze` → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(cli): analyze command + e2e"`

---

### Task 7: Quality gate + M2 exit

**Step 1:** `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace` → all green.
**Step 2: Verify M2 exit criteria (SPEC-1 §5):** `analyze` accurate on Rust + JS/TS sample repos; honors ignore/include/exclude (covered by Task 4 reuse of scanner + Task 6 E2E). Add a JS sample to the E2E if not already covered.
**Step 3: Commit** → `git add -A && git commit -m "chore(0.2.0): project analyzer milestone exit"`

---

## Open design questions (resolve during execution)
- Threshold for `TypeScript` vs `JavaScript` when both `.ts` and `.js` exist (current rule: tsconfig presence wins) — confirm.
- Whether to summarize transitive deps from lockfiles or only direct deps from manifests (current: direct only, YAGNI).
