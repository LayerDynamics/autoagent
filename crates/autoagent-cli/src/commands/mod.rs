//! CLI command handlers that render core results (SPEC-1 §3.4).

use autoagent_core::config::config_schema::{AutoAgentConfig, LlmConfig};
use autoagent_core::error::{AutoAgentError, Result};
use autoagent_core::planning::llm::config::build_provider;
use autoagent_core::planning::{plan_reader, plan_validator, plan_writer, planner};
use autoagent_core::runtime::evolve_workflow::{self, EvolveOutcome};
use autoagent_core::runtime::run_workflow::{self, RunOutcome};
use autoagent_core::safety::approval_gate::ApprovalGate;
use autoagent_core::safety::policy_engine::PolicyEngine;
use camino::{Utf8Path, Utf8PathBuf};
use std::path::PathBuf;

/// Supervised `run`: plan (or `--from`) → apply → validate → repair → report.
pub fn run(
    root: &Utf8Path,
    objective: &str,
    from: Option<PathBuf>,
    gate: &dyn ApprovalGate,
    yes: bool,
) -> Result<RunOutcome> {
    let config = AutoAgentConfig::load(root)?;
    // Resolve the write-approval decision once for the whole supervised run.
    if config.agent.require_approval_before_write && !yes {
        gate.confirm_write("planned changes")?;
    }

    if let Some(f) = from {
        let plan_path = Utf8PathBuf::from_path_buf(f)
            .map_err(|_| AutoAgentError::Plan("non-utf8 plan path".into()))?;
        run_workflow::run_with_plan(root, &plan_path, true)
    } else {
        let llm = config.llm.clone().unwrap_or_else(default_local_llm);
        let provider = build_provider(&llm)?;
        let rt = tokio::runtime::Runtime::new().map_err(AutoAgentError::Io)?;
        rt.block_on(run_workflow::run_workflow(
            root,
            objective,
            provider.as_ref(),
            true,
        ))
    }
}

/// Create a plan (via the configured LLM provider) or import one with `--from`,
/// validate it, and write the paired `.plan.json` / `.plan.md` artifacts.
pub fn plan(root: &Utf8Path, objective: &str, from: Option<PathBuf>) -> Result<Utf8PathBuf> {
    let config = AutoAgentConfig::load(root)?;
    let real_root = canonical(root);

    let plan = if let Some(f) = from {
        let import_path = Utf8PathBuf::from_path_buf(f)
            .map_err(|_| AutoAgentError::Plan("non-utf8 plan path".into()))?;
        let imported = plan_reader::read_plan(&import_path)?;
        let engine = PolicyEngine::from_config(&config, real_root.clone());
        plan_validator::validate_plan(&imported, &engine)?;
        imported
    } else {
        let llm = config.llm.clone().unwrap_or_else(default_local_llm);
        let provider = build_provider(&llm)?;
        let rt = tokio::runtime::Runtime::new().map_err(AutoAgentError::Io)?;
        rt.block_on(planner::generate_plan(
            objective,
            &config,
            root,
            provider.as_ref(),
        ))?
    };

    let (json_path, _md_path) = plan_writer::write_plan(root, &slugify(objective), &plan)?;
    Ok(json_path)
}

/// Controlled self-authoring: generate (or `--from`) a self-plan, plan-only by
/// default; `--apply` is gated by `allow_self_modification`.
pub fn evolve(
    root: &Utf8Path,
    objective: &str,
    from: Option<PathBuf>,
    apply: bool,
) -> Result<EvolveOutcome> {
    let config = AutoAgentConfig::load(root)?;
    if let Some(f) = from {
        let plan_path = Utf8PathBuf::from_path_buf(f)
            .map_err(|_| AutoAgentError::Plan("non-utf8 plan path".into()))?;
        let plan = plan_reader::read_plan(&plan_path)?;
        evolve_workflow::evolve_with_plan(root, objective, &plan, apply)
    } else {
        let llm = config.llm.clone().unwrap_or_else(default_local_llm);
        let provider = build_provider(&llm)?;
        let rt = tokio::runtime::Runtime::new().map_err(AutoAgentError::Io)?;
        rt.block_on(evolve_workflow::evolve_generated(
            root,
            objective,
            provider.as_ref(),
            apply,
        ))
    }
}

fn default_local_llm() -> LlmConfig {
    LlmConfig {
        provider: "local".into(),
        model: "llama3".into(),
        endpoint: None,
        code_egress_opt_in: false,
    }
}

fn canonical(root: &Utf8Path) -> Utf8PathBuf {
    std::fs::canonicalize(root.as_std_path())
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .unwrap_or_else(|| root.to_path_buf())
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in s.chars().take(40) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let t = out.trim_matches('-').to_string();
    if t.is_empty() {
        "plan".into()
    } else {
        t
    }
}

pub fn patch_list(root: &Utf8Path) -> Result<()> {
    let dir = root.join(".agent/patches");
    if !dir.as_std_path().is_dir() {
        println!("no patches");
        return Ok(());
    }
    let mut names: Vec<String> = std::fs::read_dir(dir.as_std_path())?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".patch"))
        .collect();
    names.sort();
    if names.is_empty() {
        println!("no patches");
    } else {
        for n in names {
            println!("{}", n.trim_end_matches(".patch"));
        }
    }
    Ok(())
}

pub fn patch_show(root: &Utf8Path, run_id: &str) -> Result<()> {
    let path = root.join(".agent/patches").join(format!("{run_id}.patch"));
    let body = std::fs::read_to_string(path.as_std_path())
        .map_err(|_| AutoAgentError::Revert(format!("no patch for run {run_id}")))?;
    print!("{body}");
    Ok(())
}

pub fn config_show(root: &Utf8Path) -> Result<()> {
    let cfg = AutoAgentConfig::load(root)?;
    let text = toml::to_string_pretty(&cfg).map_err(|e| AutoAgentError::Config(e.to_string()))?;
    print!("{text}");
    Ok(())
}

fn memory_store(root: &Utf8Path) -> Result<autoagent_core::memory::memory_store::MemoryStore> {
    let cfg = AutoAgentConfig::load(root)?;
    Ok(autoagent_core::memory::memory_store::MemoryStore::new(
        root.join(&cfg.memory.directory),
    ))
}

pub fn memory_show(root: &Utf8Path) -> Result<()> {
    let store = memory_store(root)?;
    let pm = store.load_project()?;
    println!("project: {} ({})", pm.name, pm.language);
    if let Some(pmgr) = &pm.package_manager {
        println!("package manager: {pmgr}");
    }
    println!("source files: {}", pm.source_file_count);
    let decisions = store.load_decisions()?;
    println!("decisions: {}", decisions.len());
    for d in &decisions {
        println!("  [{}] {} — {}", d.id, d.date, d.decision);
    }
    Ok(())
}

pub fn memory_rebuild(root: &Utf8Path) -> Result<()> {
    let cfg = AutoAgentConfig::load(root)?;
    let store =
        autoagent_core::memory::memory_store::MemoryStore::new(root.join(&cfg.memory.directory));
    let pm = autoagent_core::memory::project_memory::rebuild_project_memory(root, &cfg, &store)?;
    println!("rebuilt memory for {} ({})", pm.name, pm.language);
    Ok(())
}

pub fn memory_add(root: &Utf8Path, decision: &str, rationale: &str) -> Result<()> {
    let store = memory_store(root)?;
    let id = format!("d-{}", store.load_decisions()?.len() + 1);
    store.append_decision(autoagent_core::memory::schema::DecisionEntry {
        id: id.clone(),
        date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        decision: decision.to_string(),
        rationale: rationale.to_string(),
        run_id: None,
    })?;
    println!("added decision {id}");
    Ok(())
}

pub fn memory_remove(root: &Utf8Path, id: &str) -> Result<()> {
    let store = memory_store(root)?;
    if store.remove_decision(id)? {
        println!("removed decision {id}");
    } else {
        println!("no decision {id}");
    }
    Ok(())
}

pub fn tools_list(root: &Utf8Path) -> Result<()> {
    let registry = autoagent_core::plugins::with_builtins()?;
    for name in registry.tool_names() {
        println!("{name} (native)");
    }
    for manifest in autoagent_core::plugins::discover_wasm_plugins(root) {
        println!(
            "{} (wasm plugin, api {})",
            manifest.name, manifest.api_version
        );
    }
    Ok(())
}
