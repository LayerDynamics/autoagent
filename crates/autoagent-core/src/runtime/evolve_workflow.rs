//! Evolve workflow (M6) — controlled self-authoring. Plan-only by default;
//! self-apply is gated by `EvolveGuard` and isolated on a branch
//! (SPEC-1 FR-23, §3.7). "Self-authoring, not uncontrolled self-replicating."

use crate::config::config_schema::AutoAgentConfig;
use crate::error::Result;
use crate::git::branch_manager;
use crate::planning::llm::provider::LlmProvider;
use crate::planning::plan::Plan;
use crate::planning::{plan_validator, plan_writer, planner};
use crate::runtime::agent_loop;
use crate::runtime::evolve_guard::EvolveGuard;
use crate::safety::policy_engine::PolicyEngine;
use camino::{Utf8Path, Utf8PathBuf};

pub struct EvolveOutcome {
    pub plan_path: Utf8PathBuf,
    pub applied: bool,
    pub branch: Option<String>,
    pub run_id: Option<String>,
}

/// Evolve from an already-obtained plan (used by `evolve --from`).
pub fn evolve_with_plan(
    root: &Utf8Path,
    objective: &str,
    plan: &Plan,
    apply: bool,
) -> Result<EvolveOutcome> {
    let config = AutoAgentConfig::load(root)?;
    let guard = EvolveGuard::new(&config);
    guard.authorize_plan()?; // planning self is always allowed

    let engine = PolicyEngine::from_config(&config, canonical(root));
    plan_validator::validate_plan(plan, &engine)?;

    let slug = slugify(&format!("evolve-{objective}"));

    if !apply {
        let (plan_path, _md) = plan_writer::write_plan(root, &slug, plan)?;
        return Ok(EvolveOutcome {
            plan_path,
            applied: false,
            branch: None,
            run_id: None,
        });
    }

    // Apply requested → must be authorized (refused when self-mod is disabled).
    guard.authorize_apply()?;

    // Isolate on a branch BEFORE writing the plan artifact, so the clean-tree
    // check sees the pre-evolve state.
    let stamp = make_stamp(objective);
    let branch = branch_manager::branch_before_evolve(root, &stamp)?;

    let (plan_path, _md) = plan_writer::write_plan(root, &slug, plan)?;
    let run_id = agent_loop::apply(root, &plan_path, true)?;

    Ok(EvolveOutcome {
        plan_path,
        applied: true,
        branch: Some(branch),
        run_id: Some(run_id),
    })
}

/// Evolve, generating the self-plan via `provider` (used by `evolve "<objective>"`).
pub async fn evolve_generated(
    root: &Utf8Path,
    objective: &str,
    provider: &dyn LlmProvider,
    apply: bool,
) -> Result<EvolveOutcome> {
    let config = AutoAgentConfig::load(root)?;
    let plan = planner::generate_plan(objective, &config, root, provider).await?;
    evolve_with_plan(root, objective, &plan, apply)
}

fn make_stamp(objective: &str) -> String {
    format!(
        "{}-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        slugify(objective)
    )
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
        "evolve".into()
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::file_operation::{FileOperation, FileOperationKind};

    fn plan_creating(path: &str) -> Plan {
        Plan {
            objective: "improve".into(),
            summary: "s".into(),
            files_to_read: vec![],
            files_to_create: vec![crate::planning::plan::PlannedFile {
                path: path.into(),
                purpose: "x".into(),
            }],
            files_to_modify: vec![],
            operations: vec![FileOperation {
                kind: FileOperationKind::Create,
                path: path.into(),
                destination_path: None,
                reason: "r".into(),
                before_hash: None,
                after_hash: None,
                content: Some("// evolved".into()),
            }],
            validation_commands: vec![],
            risks: vec![],
            rollback_strategy: "snapshot".into(),
        }
    }

    fn workspace(self_mod: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let mut cfg =
            AutoAgentConfig::from_toml_str(&crate::config::default_config::default_toml()).unwrap();
        cfg.agent.allow_self_modification = self_mod;
        std::fs::write(root.join("Autoagent.toml"), toml::to_string(&cfg).unwrap()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"",
        )
        .unwrap();
        dir
    }

    #[test]
    fn plan_only_default_does_not_touch_source() {
        let dir = workspace(false);
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let outcome = evolve_with_plan(
            root,
            "improve planner",
            &plan_creating("crates/x.rs"),
            false,
        )
        .unwrap();
        assert!(!outcome.applied);
        assert!(outcome.plan_path.as_std_path().exists());
        assert!(!root.join("crates/x.rs").as_std_path().exists());
    }

    #[test]
    fn apply_refused_when_self_mod_disabled() {
        let dir = workspace(false);
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let res = evolve_with_plan(root, "improve", &plan_creating("crates/x.rs"), true);
        match res {
            Err(e) => assert_eq!(e.error_code(), "policy.write_not_approved"),
            Ok(_) => panic!("apply must be refused when self-mod is disabled"),
        }
        assert!(!root.join("crates/x.rs").as_std_path().exists());
    }

    #[test]
    fn applied_lands_on_evolve_branch() {
        let dir = workspace(true);
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        // Commit the seed so the tree is clean before branching.
        for args in [
            ["init"].as_slice(),
            ["add", "-A"].as_slice(),
            ["commit", "-m", "seed"].as_slice(),
        ] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(root.as_std_path())
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap();
        }
        let outcome =
            evolve_with_plan(root, "improve", &plan_creating("crates/evolved.rs"), true).unwrap();
        assert!(outcome.applied);
        let branch = outcome.branch.unwrap();
        assert!(branch.starts_with("autoagent/evolve/"));
        assert_eq!(
            crate::git::git_client::current_branch(root).unwrap(),
            branch
        );
        assert!(root.join("crates/evolved.rs").as_std_path().exists());
    }
}
