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
use crate::planning::prompt_builder::PromptKind;
use crate::planning::{plan_validator, prompt_builder};
use crate::safety::policy_engine::PolicyEngine;
use camino::{Utf8Path, Utf8PathBuf};

/// Generate a plan for the user's project (the `plan`/`run` path).
pub async fn generate_plan(
    objective: &str,
    config: &AutoAgentConfig,
    root: &Utf8Path,
    provider: &dyn LlmProvider,
) -> Result<Plan> {
    generate_plan_kind(PromptKind::Project, objective, config, root, provider).await
}

/// Generate a plan for the given planning posture. `SelfAuthoring` tells the
/// model it is changing AutoAgent's own source (used by `evolve`); the post-plan
/// policy validation is identical either way — the model never gains write
/// authority (SPEC-1 FR-22).
pub async fn generate_plan_kind(
    kind: PromptKind,
    objective: &str,
    config: &AutoAgentConfig,
    root: &Utf8Path,
    provider: &dyn LlmProvider,
) -> Result<Plan> {
    let analysis = project_analyzer::analyze(root, config)?;
    let store = crate::memory::memory_store::MemoryStore::new(root.join(&config.memory.directory));
    let decisions = crate::memory::project_memory::recent_decision_summaries(&store, 5);

    // Forward the CURRENT contents of the files the change will touch so the
    // model authors correct, surgical edits instead of replacing an unseen file
    // with a hallucinated guess. Secret/excluded files are never forwarded and
    // secret-looking lines are scrubbed (Redactor). For the local provider this
    // stays on-machine; for a cloud provider it is already gated by
    // `code_egress_opt_in` in the provider factory.
    let redactor = Redactor::new(config.workspace.exclude.clone());
    let files = gather_file_context(objective, root, &redactor, provider).await;

    let context = prompt_builder::build_kind(kind, objective, &analysis, &decisions, &files);

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

// --- file-context gathering (so the model can edit existing files correctly) ---

/// Caps that bound how much existing source is read + forwarded for one plan.
const MAX_CONTEXT_FILES: usize = 12;
const MAX_FILE_BYTES: usize = 16 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024;

/// Determine which existing files to show the model, then read them (bounded,
/// redactor-filtered, scrubbed). Two sources are merged: paths the objective
/// names verbatim (deterministic safety net) and a cheap "scout" call asking the
/// model which files it must see. Failure of either degrades gracefully to the
/// other / to none — planning still proceeds (matching prior behavior).
async fn gather_file_context(
    objective: &str,
    root: &Utf8Path,
    redactor: &Redactor,
    provider: &dyn LlmProvider,
) -> Vec<(String, String)> {
    let mut candidates: Vec<String> = objective_path_tokens(objective);

    // Scout pass: ask the model which existing files it needs to read.
    let scout = prompt_builder::build_scout(objective);
    if let Ok(raw) = provider
        .complete(&PlanRequest {
            objective: objective.to_string(),
            context: scout,
        })
        .await
    {
        candidates.extend(extract_path_list(&raw));
    }

    read_bounded(root, &candidates, redactor)
}

/// Path-like tokens (containing `/`) that appear verbatim in the objective.
fn objective_path_tokens(objective: &str) -> Vec<String> {
    objective
        .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '(' | ')' | ',' | ';'))
        .map(|t| t.trim_matches(|c: char| matches!(c, '.' | ':')))
        .filter(|t| t.contains('/') && !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// Parse the scout response: the first JSON array of strings, else empty.
fn extract_path_list(raw: &str) -> Vec<String> {
    let (start, end) = match (raw.find('['), raw.rfind(']')) {
        (Some(s), Some(e)) if e > s => (s, e),
        _ => return Vec::new(),
    };
    serde_json::from_str::<Vec<String>>(&raw[start..=end]).unwrap_or_default()
}

/// Read the candidate files within the workspace, skipping excluded/secret,
/// missing, oversized, or escaping paths; scrub secrets; cap count + total size.
fn read_bounded(
    root: &Utf8Path,
    candidates: &[String],
    redactor: &Redactor,
) -> Vec<(String, String)> {
    let real_root = canonical(root);
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut total = 0usize;

    for raw_path in candidates {
        if out.len() >= MAX_CONTEXT_FILES || total >= MAX_TOTAL_BYTES {
            break;
        }
        let rel = raw_path.trim().trim_start_matches("./");
        if rel.is_empty() || !seen.insert(rel.to_string()) || redactor.is_excluded(rel) {
            continue;
        }
        let abs = match std::fs::canonicalize(real_root.join(rel).as_std_path()) {
            Ok(p) => p,
            Err(_) => continue, // missing path
        };
        // Never read outside the workspace (defense against `../` escapes).
        if !abs.starts_with(real_root.as_std_path()) || !abs.is_file() {
            continue;
        }
        let content = match std::fs::read_to_string(&abs) {
            Ok(c) if c.len() <= MAX_FILE_BYTES => c,
            _ => continue, // binary, unreadable, or oversized
        };
        total += content.len();
        out.push((rel.to_string(), redactor.scrub(&content)));
    }
    out
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
    use std::sync::Mutex;

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

    /// Returns queued responses in call order and records each call's context,
    /// so a test can inspect exactly what the plan prompt contained.
    struct CapturingProvider {
        responses: Vec<String>,
        contexts: Mutex<Vec<String>>,
    }
    #[async_trait::async_trait]
    impl LlmProvider for CapturingProvider {
        fn name(&self) -> &str {
            "capturing"
        }
        async fn complete(&self, req: &PlanRequest) -> Result<String> {
            let mut c = self.contexts.lock().unwrap();
            let idx = c.len();
            c.push(req.context.clone());
            Ok(self.responses.get(idx).cloned().unwrap_or_default())
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
    async fn planner_forwards_existing_file_contents_to_plan_prompt() {
        let (dir, cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        // An existing source file the change must edit correctly.
        std::fs::create_dir_all(root.join("src").as_std_path()).unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "pub mod runtime;\npub mod analysis;\n",
        )
        .unwrap();
        let provider = CapturingProvider {
            responses: vec![
                // scout: name the file the planner should read
                r#"["src/lib.rs"]"#.into(),
                // plan: a valid (append) plan
                r#"{"objective":"o","summary":"s","files_to_read":[],
                  "files_to_create":[],"files_to_modify":[{"path":"src/lib.rs","purpose":"p"}],
                  "operations":[{"kind":"Append","path":"src/lib.rs","destination_path":null,
                    "reason":"r","before_hash":null,"after_hash":null,"content":"pub mod x;\n"}],
                  "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#
                    .into(),
            ],
            contexts: Mutex::new(Vec::new()),
        };
        let plan = generate_plan("add module x to src/lib.rs", &cfg, root, &provider)
            .await
            .unwrap();
        assert_eq!(plan.operations.len(), 1);
        // The PLAN prompt (2nd call) must contain the real file content, so the
        // model can edit instead of replacing an unseen file.
        let contexts = provider.contexts.lock().unwrap();
        assert_eq!(contexts.len(), 2, "expected a scout call then a plan call");
        assert!(
            contexts[1].contains("pub mod runtime;")
                && contexts[1].contains("Existing file contents"),
            "plan prompt must include the existing file's content"
        );
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
