//! 1.0.0 documentation-sync gate (M8): the README documents every command and
//! states the load-bearing safety default.

#[test]
fn readme_documents_all_commands_and_defaults() {
    let readme = include_str!("../../../README.md");
    for cmd in [
        "init", "doctor", "analyze", "plan", "apply", "run", "evolve", "patch", "revert", "memory",
        "config",
    ] {
        assert!(readme.contains(cmd), "README missing command: {cmd}");
    }
    assert!(
        readme.contains("Write operations require approval"),
        "README must state the approval-before-write default"
    );
    assert!(
        readme.contains("controlled self-authoring, not uncontrolled self-replication"),
        "README must lead with the product identity"
    );
}

#[test]
fn changelog_covers_all_milestones() {
    let changelog = include_str!("../../../CHANGELOG.md");
    for version in [
        "0.1.0", "0.2.0", "0.3.0", "0.4.0", "0.5.0", "0.6.0", "0.7.0", "1.0.0",
    ] {
        assert!(changelog.contains(version), "CHANGELOG missing {version}");
    }
}
