//! Read-only context tools for the agentic planning loop (AL-2). The model may
//! call these to navigate the repo before proposing a plan, instead of guessing.
//!
//! Every tool is **workspace-confined** (no `../` escape), **secret/exclude
//! filtered** (via `Redactor`), and bounded in size. `run_command` is gated
//! through the `PolicyEngine` — only policy-allowed commands run. These tools
//! grant NO new write authority: edits still flow through the plan → policy →
//! apply path. The model only *observes* here.

use crate::config::config_schema::AutoAgentConfig;
use crate::planning::llm::provider::{ToolCall, ToolSpec};
use crate::planning::llm::redactor::Redactor;
use crate::safety::policy_engine::PolicyEngine;
use camino::Utf8Path;
use serde_json::json;

const MAX_READ_BYTES: usize = 32 * 1024;
const MAX_GREP_HITS: usize = 60;
const MAX_LIST_ENTRIES: usize = 200;
const MAX_CMD_OUTPUT: usize = 8 * 1024;

/// The tools advertised to a tool-capable provider.
pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "read_file".into(),
            description: "Read a workspace-relative text file's contents.".into(),
            parameters: json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "grep".into(),
            description: "Find lines containing a literal substring across workspace files.".into(),
            parameters: json!({
                "type": "object",
                "properties": {"pattern": {"type": "string"}},
                "required": ["pattern"]
            }),
        },
        ToolSpec {
            name: "list_dir".into(),
            description: "List entries of a workspace-relative directory.".into(),
            parameters: json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "run_command".into(),
            description: "Run a policy-allowed command in the workspace and return its output."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {"command": {"type": "string"}},
                "required": ["command"]
            }),
        },
    ]
}

/// Execute one tool call and return the observation text fed back to the model.
/// Errors are returned as readable strings (the loop continues), never panics.
pub fn dispatch(
    call: &ToolCall,
    root: &Utf8Path,
    config: &AutoAgentConfig,
    engine: &PolicyEngine,
    approved: bool,
) -> String {
    let redactor = Redactor::new(config.workspace.exclude.clone());
    let arg = |k: &str| -> String {
        call.arguments
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    match call.name.as_str() {
        "read_file" => read_file(root, &arg("path"), &redactor),
        "grep" => grep(root, &arg("pattern"), &redactor),
        "list_dir" => list_dir(root, &arg("path"), &redactor),
        "run_command" => run_command(root, &arg("command"), engine, approved),
        other => format!("error: unknown tool '{other}'"),
    }
}

/// Resolve a workspace-relative path to an absolute path that is guaranteed to
/// stay inside the workspace (defends against `../` escapes and symlinks).
fn confined(root: &Utf8Path, rel: &str) -> Option<std::path::PathBuf> {
    let rel = rel.trim().trim_start_matches("./");
    if rel.is_empty() {
        return None;
    }
    let real_root = std::fs::canonicalize(root.as_std_path()).ok()?;
    let abs = std::fs::canonicalize(real_root.join(rel)).ok()?;
    abs.starts_with(&real_root).then_some(abs)
}

fn read_file(root: &Utf8Path, rel: &str, redactor: &Redactor) -> String {
    if redactor.is_excluded(rel) {
        return format!("error: '{rel}' is excluded/secret and cannot be read");
    }
    let abs = match confined(root, rel) {
        Some(p) if p.is_file() => p,
        _ => return format!("error: '{rel}' not found in the workspace"),
    };
    match std::fs::read_to_string(&abs) {
        Ok(c) if c.len() <= MAX_READ_BYTES => redactor.scrub(&c),
        Ok(_) => format!("error: '{rel}' is larger than {MAX_READ_BYTES} bytes"),
        Err(e) => format!("error: reading '{rel}': {e}"),
    }
}

fn list_dir(root: &Utf8Path, rel: &str, redactor: &Redactor) -> String {
    let rel = if rel.trim().is_empty() { "." } else { rel };
    let abs = match confined(root, rel) {
        Some(p) if p.is_dir() => p,
        _ => return format!("error: directory '{rel}' not found in the workspace"),
    };
    let mut names: Vec<String> = match std::fs::read_dir(&abs) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                if e.path().is_dir() {
                    format!("{n}/")
                } else {
                    n
                }
            })
            .filter(|n| !redactor.is_excluded(n.trim_end_matches('/')))
            .take(MAX_LIST_ENTRIES)
            .collect(),
        Err(e) => return format!("error: listing '{rel}': {e}"),
    };
    names.sort();
    if names.is_empty() {
        format!("(empty) {rel}")
    } else {
        names.join("\n")
    }
}

fn grep(root: &Utf8Path, pattern: &str, redactor: &Redactor) -> String {
    if pattern.is_empty() {
        return "error: empty grep pattern".into();
    }
    let real_root = match std::fs::canonicalize(root.as_std_path()) {
        Ok(p) => p,
        Err(e) => return format!("error: {e}"),
    };
    let mut hits: Vec<String> = Vec::new();
    let mut stack = vec![real_root.clone()];
    while let Some(dir) = stack.pop() {
        if hits.len() >= MAX_GREP_HITS {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.filter_map(|e| e.ok()) {
            let path = entry.path();
            let rel = path
                .strip_prefix(&real_root)
                .ok()
                .and_then(|p| p.to_str())
                .unwrap_or("")
                .to_string();
            if rel.is_empty() || redactor.is_excluded(&rel) {
                continue;
            }
            // Skip the usual heavy/irrelevant trees even if not in excludes.
            if rel.starts_with(".git/") || rel.starts_with("target/") || rel == ".git" {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(content) = std::fs::read_to_string(&path) {
                for (i, line) in content.lines().enumerate() {
                    if line.contains(pattern) {
                        hits.push(format!("{rel}:{}: {}", i + 1, line.trim()));
                        if hits.len() >= MAX_GREP_HITS {
                            break;
                        }
                    }
                }
            }
        }
    }
    if hits.is_empty() {
        format!("no matches for '{pattern}'")
    } else {
        hits.join("\n")
    }
}

fn run_command(root: &Utf8Path, command: &str, engine: &PolicyEngine, approved: bool) -> String {
    // Allow-listed commands always run; a clean unknown command (e.g. installing
    // or invoking a tool the task needs) runs only when the run is approved;
    // hard-blocked / unsafe commands never run.
    if let Err(e) = engine.authorize_command(command, approved) {
        return format!("error: command not allowed by policy: {e}");
    }
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return "error: empty command".into();
    }
    match std::process::Command::new(parts[0])
        .args(&parts[1..])
        .current_dir(root.as_std_path())
        .output()
    {
        Ok(out) => {
            let mut s = format!("exit: {}\n", out.status.code().unwrap_or(-1));
            s.push_str(&String::from_utf8_lossy(&out.stdout));
            s.push_str(&String::from_utf8_lossy(&out.stderr));
            s.truncate(MAX_CMD_OUTPUT);
            s
        }
        Err(e) => format!("error: running '{command}': {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::config_schema::AutoAgentConfig;
    use crate::config::default_config;

    fn fixture() -> (tempfile::TempDir, AutoAgentConfig) {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("Autoagent.toml"), default_config::default_toml()).unwrap();
        std::fs::create_dir_all(root.join("src").as_std_path()).unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn alpha() {}\npub fn beta() {}\n",
        )
        .unwrap();
        std::fs::write(root.join(".env"), "API_KEY=sk-secret\n").unwrap();
        let cfg = AutoAgentConfig::load(root).unwrap();
        (dir, cfg)
    }

    fn root_of(dir: &tempfile::TempDir) -> &camino::Utf8Path {
        camino::Utf8Path::from_path(dir.path()).unwrap()
    }

    fn call(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "1".into(),
            name: name.into(),
            arguments: args,
        }
    }

    #[test]
    fn read_file_returns_content_and_blocks_secrets_and_escapes() {
        let (dir, cfg) = fixture();
        let root = root_of(&dir);
        let engine = PolicyEngine::from_config(&cfg, root.to_path_buf());

        let ok = dispatch(
            &call("read_file", json!({"path": "src/lib.rs"})),
            root,
            &cfg,
            &engine,
            false,
        );
        assert!(ok.contains("pub fn alpha"));

        // Secret file is refused.
        let secret = dispatch(
            &call("read_file", json!({"path": ".env"})),
            root,
            &cfg,
            &engine,
            false,
        );
        assert!(
            secret.starts_with("error:"),
            "secret must be refused: {secret}"
        );

        // Path escape is refused.
        let escape = dispatch(
            &call("read_file", json!({"path": "../../etc/hosts"})),
            root,
            &cfg,
            &engine,
            false,
        );
        assert!(
            escape.starts_with("error:"),
            "escape must be refused: {escape}"
        );
    }

    #[test]
    fn grep_finds_matches_and_list_dir_lists() {
        let (dir, cfg) = fixture();
        let root = root_of(&dir);
        let engine = PolicyEngine::from_config(&cfg, root.to_path_buf());

        let g = dispatch(
            &call("grep", json!({"pattern": "fn beta"})),
            root,
            &cfg,
            &engine,
            false,
        );
        assert!(g.contains("src/lib.rs:") && g.contains("fn beta"));

        let l = dispatch(
            &call("list_dir", json!({"path": "src"})),
            root,
            &cfg,
            &engine,
            false,
        );
        assert!(l.contains("lib.rs"));
    }

    #[test]
    fn run_command_is_policy_gated() {
        let (dir, cfg) = fixture();
        let root = root_of(&dir);
        let engine = PolicyEngine::from_config(&cfg, root.to_path_buf());

        // A blocked command never runs.
        let blocked = dispatch(
            &call("run_command", json!({"command": "sudo rm -rf /"})),
            root,
            &cfg,
            &engine,
            false,
        );
        assert!(blocked.contains("not allowed by policy"), "got: {blocked}");

        // An allowed command runs and returns output.
        let ok = dispatch(
            &call("run_command", json!({"command": "git status"})),
            root,
            &cfg,
            &engine,
            false,
        );
        assert!(ok.contains("exit:"), "allowed command should run: {ok}");

        // An unknown but clean command (a tool the task needs) is REFUSED without
        // approval and RUNS with approval — this is how the agent pursues tools.
        let needed = call("run_command", json!({"command": "echo aatool"}));
        let refused = dispatch(&needed, root, &cfg, &engine, false);
        assert!(
            refused.contains("not allowed by policy"),
            "unknown command must need approval: {refused}"
        );
        let approved = dispatch(&needed, root, &cfg, &engine, true);
        assert!(
            approved.contains("exit:") && approved.contains("aatool"),
            "approved unknown command must run: {approved}"
        );
    }

    #[test]
    fn unknown_tool_is_reported() {
        let (dir, cfg) = fixture();
        let root = root_of(&dir);
        let engine = PolicyEngine::from_config(&cfg, root.to_path_buf());
        let r = dispatch(&call("frobnicate", json!({})), root, &cfg, &engine, false);
        assert!(r.contains("unknown tool"));
    }
}
