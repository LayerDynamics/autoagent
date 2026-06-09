//! CLI command handlers that render core results (SPEC-1 §3.4).

use autoagent_core::config::config_schema::{AutoAgentConfig, LlmConfig};
use autoagent_core::error::{AutoAgentError, Result};
use autoagent_core::planning::llm::config::build_provider;
use autoagent_core::planning::{plan_reader, plan_validator, plan_writer, planner};
use autoagent_core::safety::policy_engine::PolicyEngine;
use camino::{Utf8Path, Utf8PathBuf};
use std::path::PathBuf;

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
