//! Reproducible run sessions — the "reproducible autonomous loop".
//!
//! AutoAgent has two loop kinds. The SHORT ITERATIVE loop (the agentic
//! read-edit-observe loop and the repair loop) is live and model-driven: tight,
//! fast, nondeterministic. The REPRODUCIBLE AUTONOMOUS loop is the outer loop:
//! every plan an autonomous `run` applies is recorded, in order, as a *session*
//! — so the exact same multi-step change can be REPLAYED deterministically with
//! no model and no nondeterminism, reproducing the result bit-for-bit.
//!
//! Replay reuses the same policy-gated, snapshotted, reversible apply path, so a
//! reproduced run is as safe and reversible as the original.

use crate::config::config_schema::AutoAgentConfig;
use crate::error::{AutoAgentError, Result};
use crate::planning::plan::Plan;
use crate::runtime::run_state::RunState;
use crate::runtime::run_workflow::{self, RunOutcome};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

/// The replayable record of a run: an ordered list of the plans it applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionManifest {
    pub session_id: String,
    pub objective: String,
    pub created: String,
    pub steps: usize,
}

/// `.agent/sessions` (derived relative to the configured runs directory so it
/// follows a relocated `.agent`).
fn sessions_dir(root: &Utf8Path, config: &AutoAgentConfig) -> Utf8PathBuf {
    let runs = Utf8Path::new(&config.runs.directory);
    let base = runs.parent().unwrap_or_else(|| Utf8Path::new(".agent"));
    root.join(base).join("sessions")
}

fn step_file(index: usize) -> String {
    format!("step-{index:03}.plan.json")
}

/// Record the ordered plans a run applied as a replayable session; returns its
/// id. Each step is the self-contained plan whose changes are in the final tree.
pub fn record(
    root: &Utf8Path,
    config: &AutoAgentConfig,
    objective: &str,
    plans: &[Plan],
) -> Result<String> {
    let created = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let session_id = format!("{created}-{}", slug(objective));
    let dir = sessions_dir(root, config).join(&session_id);
    std::fs::create_dir_all(dir.as_std_path())?;

    for (i, plan) in plans.iter().enumerate() {
        let json =
            serde_json::to_string_pretty(plan).map_err(|e| AutoAgentError::Serde(e.to_string()))?;
        std::fs::write(dir.join(step_file(i + 1)).as_std_path(), json)?;
    }

    let manifest = SessionManifest {
        session_id: session_id.clone(),
        objective: objective.to_string(),
        created,
        steps: plans.len(),
    };
    std::fs::write(
        dir.join("session.json").as_std_path(),
        serde_json::to_string_pretty(&manifest)
            .map_err(|e| AutoAgentError::Serde(e.to_string()))?,
    )?;
    Ok(session_id)
}

/// Load a session manifest.
pub fn load(root: &Utf8Path, session_id: &str) -> Result<SessionManifest> {
    let config = AutoAgentConfig::load(root)?;
    let path = sessions_dir(root, &config)
        .join(session_id)
        .join("session.json");
    let text = std::fs::read_to_string(path.as_std_path())
        .map_err(|e| AutoAgentError::Revert(format!("cannot read session {session_id}: {e}")))?;
    serde_json::from_str(&text).map_err(|e| AutoAgentError::Serde(e.to_string()))
}

/// Replay a recorded session deterministically: apply each step's plan in order
/// through the normal apply+validate path — no model. Reproduces the result.
/// Stops and returns early if any step does not reach `Completed`.
pub fn replay(root: &Utf8Path, session_id: &str) -> Result<RunOutcome> {
    let config = AutoAgentConfig::load(root)?;
    let dir = sessions_dir(root, &config).join(session_id);
    let manifest = load(root, session_id)?;
    if manifest.steps == 0 {
        return Err(AutoAgentError::Validation(format!(
            "session {session_id} has no steps to replay"
        )));
    }
    let mut last: Option<RunOutcome> = None;
    for i in 1..=manifest.steps {
        let plan_path = dir.join(step_file(i));
        // Replay reuses the supervised apply+validate path (no provider, no
        // repair): same plan → same operations → same validation.
        let outcome = run_workflow::run_with_plan(root, &plan_path, true)?;
        let passed = matches!(outcome.final_state, RunState::Completed);
        last = Some(outcome);
        if !passed {
            break; // a step diverged; surface it rather than press on
        }
    }
    last.ok_or_else(|| AutoAgentError::Validation("session produced no outcome".into()))
}

fn slug(s: &str) -> String {
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
        "session".into()
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::file_operation::{FileOperation, FileOperationKind};
    use crate::planning::plan::{Plan, PlannedFile};

    fn create_plan(path: &str, content: &str) -> Plan {
        Plan {
            objective: "o".into(),
            summary: "s".into(),
            files_to_read: vec![],
            files_to_create: vec![PlannedFile {
                path: path.into(),
                purpose: "p".into(),
            }],
            files_to_modify: vec![],
            operations: vec![FileOperation {
                kind: FileOperationKind::Create,
                path: path.into(),
                destination_path: None,
                reason: "r".into(),
                before_hash: None,
                after_hash: None,
                content: Some(content.into()),
                anchor: None,
            }],
            validation_commands: vec![],
            risks: vec![],
            rollback_strategy: "snapshot".into(),
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
        let cfg = AutoAgentConfig::load(root).unwrap();
        (dir, cfg)
    }

    #[test]
    fn record_then_replay_reproduces_the_changes() {
        let (dir, cfg) = workspace();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();

        // A two-step autonomous session.
        let plans = vec![
            create_plan("crates/a.rs", "// a\n"),
            create_plan("crates/b.rs", "// b\n"),
        ];
        let id = record(root, &cfg, "build two files", &plans).unwrap();

        // The session is self-contained on disk.
        assert!(load(root, &id).unwrap().steps == 2);

        // Replay on the (clean) workspace reproduces BOTH files deterministically.
        let outcome = replay(root, &id).unwrap();
        assert!(matches!(outcome.final_state, RunState::Completed));
        assert_eq!(
            std::fs::read_to_string(root.join("crates/a.rs")).unwrap(),
            "// a\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("crates/b.rs")).unwrap(),
            "// b\n"
        );
    }
}
