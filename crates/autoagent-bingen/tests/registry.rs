//! The surface registry is the contract every backend + stub is generated from.
//! These tests freeze full-CLI parity and the privilege/async classification.

use autoagent_bingen::bind::{Kind, Privilege, SURFACE};

#[test]
fn registry_covers_full_cli_parity() {
    let names: Vec<&str> = SURFACE.iter().map(|s| s.name).collect();
    for required in [
        "init",
        "doctor",
        "analyze",
        "plan",
        "apply",
        "run",
        "evolve",
        "revert",
        "patch_list",
        "patch_show",
        "config_show",
        "memory_show",
        "tools_list",
        "version",
    ] {
        assert!(
            names.contains(&required),
            "missing surface symbol: {required}"
        );
    }
}

#[test]
fn mutating_ops_marked_mutate() {
    for s in SURFACE
        .iter()
        .filter(|s| ["apply", "run", "evolve", "revert"].contains(&s.name))
    {
        assert!(
            matches!(s.privilege, Privilege::Mutate),
            "{} must be Mutate",
            s.name
        );
    }
}

#[test]
fn async_ops_marked_async() {
    for s in SURFACE
        .iter()
        .filter(|s| ["run", "evolve"].contains(&s.name))
    {
        assert!(matches!(s.kind, Kind::Async), "{} must be Async", s.name);
    }
}
