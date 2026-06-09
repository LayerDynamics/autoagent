//! 1.0.0 run-folder contract (SPEC-1 FR-9 + FR-10): every applied run produces
//! the complete run folder, and events mirror to the workspace-level log.

use autoagent_core::runtime::agent_loop;
use camino::Utf8Path;

#[test]
fn apply_produces_complete_run_folder_and_workspace_log() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    std::fs::write(
        root.join("Autoagent.toml"),
        autoagent_core::config::default_config::default_toml(),
    )
    .unwrap();
    std::fs::write(
        root.join("p.json"),
        r#"{"objective":"contract","summary":"s","files_to_read":[],
        "files_to_create":[{"path":"crates/x.rs","purpose":"p"}],"files_to_modify":[],
        "operations":[{"kind":"Create","path":"crates/x.rs","destination_path":null,"reason":"r",
          "before_hash":null,"after_hash":null,"content":"// x"}],
        "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
    )
    .unwrap();

    let plan = root.join("p.json");
    let run_id = agent_loop::apply(root, &plan, true).unwrap();
    let run_dir = root.join(format!(".agent/runs/{run_id}"));

    // FR-9: the full run-folder contract.
    for f in [
        "run.json",
        "objective.md",
        "plan.md",
        "events.jsonl",
        "file-operations.json",
        "validation-report.md",
        "summary.md",
    ] {
        assert!(
            run_dir.join(f).as_std_path().exists(),
            "run folder missing required file: {f}"
        );
    }
    assert!(run_dir.join("before").as_std_path().is_dir());
    assert!(run_dir.join("after").as_std_path().is_dir());

    // file-operations.json is the structured operation list.
    let ops: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(run_dir.join("file-operations.json").as_std_path()).unwrap(),
    )
    .unwrap();
    assert_eq!(ops.as_array().unwrap().len(), 1);

    // FR-10: events also mirror to the workspace-level aggregate log.
    let ws_log = root.join(".agent/logs/events.jsonl");
    assert!(
        ws_log.as_std_path().exists(),
        "workspace events.jsonl missing"
    );
    let body = std::fs::read_to_string(ws_log.as_std_path()).unwrap();
    assert!(body.lines().any(|l| l.contains("run_started")));
    assert!(body.lines().any(|l| l.contains(&run_id)));
}
