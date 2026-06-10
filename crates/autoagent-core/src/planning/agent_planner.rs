//! Agentic planning loop (AL-4). When the provider supports tool-use, the model
//! may call the read-only context tools (`runtime::agent_tools`) — `read_file`,
//! `grep`, `list_dir`, `run_command` — to navigate the repo before emitting its
//! plan, instead of guessing from metadata. Falls back to the proven one-shot,
//! schema-constrained planner when the provider has no tool support or the loop
//! fails to produce a valid plan. The returned plan is policy-validated exactly
//! like the one-shot path — the model never gains write authority.

use crate::analysis::project_analyzer;
use crate::config::config_schema::AutoAgentConfig;
use crate::error::{AutoAgentError, Result};
use crate::planning::llm::provider::{LlmProvider, Message, Turn};
use crate::planning::plan::Plan;
use crate::planning::prompt_builder::{self, PromptKind};
use crate::planning::{plan_validator, planner};
use crate::runtime::agent_tools;
use crate::safety::policy_engine::PolicyEngine;
use camino::{Utf8Path, Utf8PathBuf};

/// Max conversation rounds (tool batches + correction attempts) before giving up
/// and falling back to one-shot planning.
const MAX_ROUNDS: u32 = 10;

/// Generate a plan, preferring the agentic tool-use loop when available.
/// `approved` is the run's command-approval decision: when true, the model's
/// `run_command` tool may run clean unknown commands (so it can pursue the tools
/// the task needs); when false it is limited to allow-listed commands.
pub async fn generate_plan_agentic(
    kind: PromptKind,
    objective: &str,
    config: &AutoAgentConfig,
    root: &Utf8Path,
    provider: &dyn LlmProvider,
    approved: bool,
) -> Result<Plan> {
    if provider.supports_tools() {
        if let Ok(plan) = agentic_loop(kind, objective, config, root, provider, approved).await {
            return Ok(plan);
        }
        // Any failure (no tool support at runtime, malformed plan, budget
        // exhausted) degrades to the proven one-shot path.
    }
    planner::generate_plan_kind(kind, objective, config, root, provider).await
}

async fn agentic_loop(
    kind: PromptKind,
    objective: &str,
    config: &AutoAgentConfig,
    root: &Utf8Path,
    provider: &dyn LlmProvider,
    approved: bool,
) -> Result<Plan> {
    let analysis = project_analyzer::analyze(root, config)?;
    let store = crate::memory::memory_store::MemoryStore::new(root.join(&config.memory.directory));
    let decisions = crate::memory::project_memory::recent_decision_summaries(&store, 5);
    let schema = prompt_builder::build_kind(kind, objective, &analysis, &decisions, &[]);
    let tools = agent_tools::tool_specs();
    let engine = PolicyEngine::from_config(config, canonical(root));

    let mut msgs = vec![
        Message {
            role: "system".into(),
            content: format!(
                "{schema}\n\nYou may FIRST call the provided tools (read_file, grep, list_dir, \
                 run_command) to inspect the actual repository before deciding. When you are ready, \
                 reply with ONLY the plan JSON object — no prose, no tool call."
            ),
        },
        Message {
            role: "user".into(),
            content: format!("Objective: {objective}"),
        },
    ];

    for _ in 0..MAX_ROUNDS {
        match provider.converse(&msgs, &tools).await? {
            Turn::ToolCalls(calls) => {
                for call in &calls {
                    let observation = agent_tools::dispatch(call, root, config, &engine, approved);
                    msgs.push(Message {
                        role: "assistant".into(),
                        content: format!("(tool call) {} {}", call.name, call.arguments),
                    });
                    msgs.push(Message {
                        role: "tool".into(),
                        content: format!("[{}]\n{}", call.name, observation),
                    });
                }
            }
            Turn::Final(text) => match parse_validated(&text, &engine) {
                Ok(plan) => return Ok(plan),
                Err(e) => {
                    // Give the model one chance to correct an invalid plan with
                    // the exact error, then keep looping within budget.
                    msgs.push(Message {
                        role: "assistant".into(),
                        content: text,
                    });
                    msgs.push(Message {
                        role: "user".into(),
                        content: format!(
                            "That was not an applicable plan: {e}. Reply with ONLY a corrected \
                             plan JSON object."
                        ),
                    });
                }
            },
        }
    }
    Err(AutoAgentError::Plan(
        "agentic loop did not produce a valid plan within the round budget".into(),
    ))
}

fn parse_validated(text: &str, engine: &PolicyEngine) -> Result<Plan> {
    let json = planner::extract_json(text)
        .ok_or_else(|| AutoAgentError::Plan("agentic: response contained no JSON object".into()))?;
    let plan: Plan = serde_json::from_str(json)
        .map_err(|e| AutoAgentError::Plan(format!("agentic: plan JSON invalid: {e}")))?;
    plan_validator::validate_plan(&plan, engine)?;
    Ok(plan)
}

fn canonical(root: &Utf8Path) -> Utf8PathBuf {
    std::fs::canonicalize(root.as_std_path())
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .unwrap_or_else(|| root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::llm::provider::{PlanRequest, ToolCall, ToolSpec};
    use std::sync::Mutex;

    /// A provider that first asks for a tool, then returns a final plan — proving
    /// the loop executes tools and feeds observations back before planning.
    struct ToolingProvider {
        turns: Mutex<Vec<Turn>>,
        saw_tool_observation: Mutex<bool>,
    }
    #[async_trait::async_trait]
    impl LlmProvider for ToolingProvider {
        fn name(&self) -> &str {
            "tooling"
        }
        async fn complete(&self, _req: &PlanRequest) -> Result<String> {
            Ok(String::new())
        }
        fn supports_tools(&self) -> bool {
            true
        }
        async fn converse(&self, msgs: &[Message], _tools: &[ToolSpec]) -> Result<Turn> {
            // Record whether a tool observation made it back into the history.
            if msgs.iter().any(|m| m.role == "tool") {
                *self.saw_tool_observation.lock().unwrap() = true;
            }
            Ok(self.turns.lock().unwrap().remove(0))
        }
    }

    fn workspace() -> (tempfile::TempDir, AutoAgentConfig) {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            root.join("Autoagent.toml"),
            crate::config::default_config::default_toml(),
        )
        .unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src").as_std_path()).unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn x() {}\n").unwrap();
        let cfg = AutoAgentConfig::load(root).unwrap();
        (dir, cfg)
    }

    const PLAN: &str = r#"{"objective":"o","summary":"s","files_to_read":[],
      "files_to_create":[{"path":"crates/x.rs","purpose":"p"}],"files_to_modify":[],
      "operations":[{"kind":"Create","path":"crates/x.rs","destination_path":null,
        "reason":"r","before_hash":null,"after_hash":null,"content":"// x"}],
      "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#;

    #[tokio::test]
    async fn agentic_loop_runs_a_tool_then_plans() {
        let (dir, cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let provider = ToolingProvider {
            turns: Mutex::new(vec![
                Turn::ToolCalls(vec![ToolCall {
                    id: "1".into(),
                    name: "read_file".into(),
                    arguments: serde_json::json!({"path": "src/lib.rs"}),
                }]),
                Turn::Final(PLAN.into()),
            ]),
            saw_tool_observation: Mutex::new(false),
        };
        let plan = generate_plan_agentic(PromptKind::Project, "do x", &cfg, root, &provider, true)
            .await
            .unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert!(
            *provider.saw_tool_observation.lock().unwrap(),
            "the tool result must be fed back into the conversation"
        );
    }

    #[tokio::test]
    async fn falls_back_to_one_shot_when_no_tool_support() {
        let (dir, cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        // A non-tool provider: generate_plan_agentic must use the one-shot path.
        struct OneShot;
        #[async_trait::async_trait]
        impl LlmProvider for OneShot {
            fn name(&self) -> &str {
                "oneshot"
            }
            async fn complete(&self, _req: &PlanRequest) -> Result<String> {
                Ok(PLAN.into())
            }
        }
        let plan = generate_plan_agentic(PromptKind::Project, "do x", &cfg, root, &OneShot, false)
            .await
            .unwrap();
        assert_eq!(plan.operations.len(), 1);
    }
}
