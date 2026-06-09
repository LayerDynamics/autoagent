//! Planner (M3) — orchestrates context → provider → parse → MANDATORY
//! post-validation. The model only proposes a plan; it never gains write
//! authority. Any plan that violates policy surfaces a policy error instead of
//! a Plan (SPEC-1 FR-22).

use crate::analysis::project_analyzer;
use crate::config::config_schema::AutoAgentConfig;
use crate::error::{AutoAgentError, Result};
use crate::planning::llm::provider::{LlmProvider, PlanRequest};
use crate::planning::llm::redactor::Redactor;
use crate::planning::plan::Plan;
use crate::planning::{plan_validator, prompt_builder};
use crate::safety::policy_engine::PolicyEngine;
use camino::{Utf8Path, Utf8PathBuf};

pub async fn generate_plan(
    objective: &str,
    config: &AutoAgentConfig,
    root: &Utf8Path,
    provider: &dyn LlmProvider,
) -> Result<Plan> {
    let analysis = project_analyzer::analyze(root, config)?;
    let store = crate::memory::memory_store::MemoryStore::new(root.join(&config.memory.directory));
    let decisions = crate::memory::project_memory::recent_decision_summaries(&store, 5);
    let context = prompt_builder::build(objective, &analysis, &decisions);

    let raw = provider
        .complete(&PlanRequest {
            objective: objective.to_string(),
            context,
        })
        .await?;

    let json = extract_json(&raw)
        .ok_or_else(|| AutoAgentError::Plan("provider returned no JSON object".into()))?;
    let plan: Plan = serde_json::from_str(json)
        .map_err(|e| AutoAgentError::Plan(format!("provider JSON invalid: {e}")))?;

    // Defense in depth: refuse a plan that would read excluded/secret files.
    let redactor = Redactor::new(config.workspace.exclude.clone());
    for f in &plan.files_to_read {
        if redactor.is_excluded(f.as_str()) {
            return Err(AutoAgentError::Plan(format!(
                "plan would read excluded/secret file: {f}"
            )));
        }
    }

    // The model never gets write authority — every op is policy-validated.
    let engine = PolicyEngine::from_config(config, canonical(root));
    plan_validator::validate_plan(&plan, &engine)?;
    Ok(plan)
}

fn extract_json(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    (end > start).then(|| &s[start..=end])
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
    use crate::planning::llm::provider::LlmProvider;

    struct FakeProvider(String);

    #[async_trait::async_trait]
    impl LlmProvider for FakeProvider {
        fn name(&self) -> &str {
            "fake"
        }
        async fn complete(&self, _req: &PlanRequest) -> Result<String> {
            Ok(self.0.clone())
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
        let cfg = AutoAgentConfig::load(root).unwrap();
        (dir, cfg)
    }

    #[tokio::test]
    async fn planner_returns_validated_plan() {
        let (dir, cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let good = FakeProvider(
            r#"Here is the plan: {"objective":"add","summary":"s","files_to_read":[],
          "files_to_create":[{"path":"crates/x.rs","purpose":"p"}],"files_to_modify":[],
          "operations":[{"kind":"Create","path":"crates/x.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"x"}],
          "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#
                .into(),
        );
        let plan = generate_plan("add", &cfg, root, &good).await.unwrap();
        assert_eq!(plan.operations.len(), 1);
    }

    #[tokio::test]
    async fn planner_rejects_blocked_path_op() {
        let (dir, cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let bad = FakeProvider(
            r#"{"objective":"x","summary":"s","files_to_read":[],
          "files_to_create":[],"files_to_modify":[],
          "operations":[{"kind":"Write","path":".git/config","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"x"}],
          "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#
                .into(),
        );
        let res = generate_plan("x", &cfg, root, &bad).await;
        match res {
            Err(e) => assert_eq!(e.error_code(), "policy.blocked_path"),
            Ok(_) => panic!("blocked-path op must be refused, not returned as a plan"),
        }
    }

    #[tokio::test]
    async fn planner_rejects_reading_secret_file() {
        let (dir, cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let bad = FakeProvider(
            r#"{"objective":"x","summary":"s","files_to_read":[".env"],
          "files_to_create":[],"files_to_modify":[],
          "operations":[{"kind":"Create","path":"crates/x.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"x"}],
          "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#
                .into(),
        );
        assert!(generate_plan("x", &cfg, root, &bad).await.is_err());
    }
}
