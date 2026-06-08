//! CLI command handlers that render core results (SPEC-1 §3.4).

use autoagent_core::config::config_schema::AutoAgentConfig;
use autoagent_core::error::{AutoAgentError, Result};
use camino::Utf8Path;

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
