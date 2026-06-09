//! 1.0.0 audit-trail completeness (M8, SPEC-1 §2.2 audit + §3.4.3 integrity):
//! every applied operation is recorded in events.jsonl with a monotonic,
//! gap-free per-run sequence.

use autoagent_core::runtime::agent_loop;
use camino::Utf8Path;

#[test]
fn every_operation_appears_in_events_with_monotonic_seq() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    std::fs::write(
        root.join("Autoagent.toml"),
        autoagent_core::config::default_config::default_toml(),
    )
    .unwrap();

    // Three operations in one plan.
    std::fs::write(
        root.join("p.json"),
        r#"{"objective":"multi","summary":"s","files_to_read":[],
        "files_to_create":[],"files_to_modify":[],
        "operations":[
          {"kind":"Create","path":"crates/a.rs","destination_path":null,"reason":"r","before_hash":null,"after_hash":null,"content":"1"},
          {"kind":"Create","path":"crates/b.rs","destination_path":null,"reason":"r","before_hash":null,"after_hash":null,"content":"2"},
          {"kind":"Create","path":"crates/c.rs","destination_path":null,"reason":"r","before_hash":null,"after_hash":null,"content":"3"}
        ],
        "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#,
    )
    .unwrap();

    let plan = root.join("p.json");
    let run_id = agent_loop::apply(root, &plan, true).unwrap();

    let events_path = root.join(format!(".agent/runs/{run_id}/events.jsonl"));
    let body = std::fs::read_to_string(events_path.as_std_path()).unwrap();
    let events: Vec<serde_json::Value> = body
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    // Every operation is recorded.
    let applied = events
        .iter()
        .filter(|e| e["type"] == "operation_applied")
        .count();
    assert_eq!(applied, 3, "all three operations must be recorded");

    // Terminal completion is recorded.
    assert!(events.iter().any(|e| e["type"] == "run_completed"));

    // seq is monotonic and gap-free (1..=N).
    let seqs: Vec<u64> = events.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    assert_eq!(seqs.first(), Some(&1));
    for (i, s) in seqs.iter().enumerate() {
        assert_eq!(*s, (i as u64) + 1, "seq gap at index {i}");
    }

    // Every event carries the schema version.
    assert!(events.iter().all(|e| e["schema_version"] == 1));
}
