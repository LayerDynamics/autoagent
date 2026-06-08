# M8 — 1.0.0 Stable Release Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `lore:execute` to implement this plan task-by-task.
> **Scope guard:** Do ONLY what is listed here. If you discover adjacent issues, note them as a TODO and continue. Do NOT fix them.

**Goal:** Freeze AutoAgent at 1.0.0 — stable CLI, config schema, and plan schema; verified reversible patches, policy enforcement, and audit logging; documented and release-ready.
**Architecture:** No new subsystems. This milestone hardens, versions, documents, and exhaustively verifies the M1–M7 surface, then freezes the public contracts.
**Tech Stack:** Rust 2021; existing deps. Adds schema-export tooling (`schemars`) and release CI.
**Practices:** TDD (regression-test-first for every hardening fix), contract-first (schema versioning), typed-first.
**Required skills:** none.
**Prerequisite:** **M1–M7 complete.**
**Design status:** ⚠️ **PARTIALLY PROPOSED.** SPEC-1 §13 fixes the deliverables (stable CLI/config/plan schema, reversible patches, policy enforcement, audit logging) and §14 fixes the principles. The *versioning mechanism*, schema-freeze approach, and release process below are design decisions to confirm. The verification targets trace directly to SPEC-1 §6 success criteria and §7 risks.

**Contracts FROZEN at this milestone (breaking changes forbidden after 1.0.0):** `Autoagent.toml` schema, the JSON `Plan` schema (SPEC-1 §3.4.1), `run.json` (§3.4.2), the `events.jsonl` catalog (§3.4.3), `AutoAgentError` exit codes (§3.11), and the CLI command surface (§3.4).

---

### Task 1: Schema versioning + freeze markers

**Files:**
- Create: `crates/autoagent-core/src/schema_version.rs`
- Modify: `config_schema.rs`, `plan.rs`, run.json writer, event emitter to stamp a `schema_version`
- Modify: `crates/autoagent-core/src/lib.rs`

**Step 1: Write the failing test**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn schema_version_is_one() { assert_eq!(SCHEMA_VERSION, 1); }
    #[test] fn run_json_carries_schema_version() {
        let v = sample_run_json_value();
        assert_eq!(v["schema_version"], 1);
    }
    #[test] fn plan_with_unknown_future_version_is_rejected() {
        // a plan claiming schema_version=2 must be refused by a 1.0 binary
        assert!(crate::planning::plan_reader::accepts_version(2).is_err());
    }
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `pub const SCHEMA_VERSION: u32 = 1;`. Add optional `schema_version` (default 1) to `Plan`, `run.json`, and the event envelope. `plan_reader` rejects a plan whose `schema_version > SCHEMA_VERSION` with `AutoAgentError::Plan`. Backward-compatible: absent field assumes 1.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(core): schema versioning (v1) across plan/run/events"`

---

### Task 2: Exported JSON Schemas (machine-checkable contracts)

**Files:**
- Modify: `crates/autoagent-core/Cargo.toml` (add `schemars`)
- Derive `JsonSchema` on `Plan`, `FileOperation`, config structs
- Create: `crates/autoagent-cli/src/commands/schema.rs` (`schema export <plan|config|run>`)
- Create: `schemas/plan.schema.json`, `schemas/config.schema.json` (checked-in golden files)
- Create: `crates/autoagent-core/tests/schema_snapshot.rs`

**Step 1: Write the failing test**
```rust
#[test] fn exported_plan_schema_matches_golden() {
    let generated = autoagent_core::planning::plan::plan_json_schema();
    let golden = include_str!("../../../schemas/plan.schema.json");
    assert_eq!(generated.trim(), golden.trim(),
        "Plan schema changed — this is a BREAKING change after 1.0.0; update golden only with a version bump");
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write minimal implementation** — `schemars`-derive on the contract types; `plan_json_schema()` returns the pretty JSON Schema; `schema export` CLI writes it. Generate the golden files once, commit them. The snapshot test is the **freeze enforcer**: any post-1.0 change to a frozen schema fails CI, forcing a deliberate version bump.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "feat(core): exported JSON Schemas + freeze snapshot tests"`

---

### Task 3: Reversibility & policy verification suite (success criteria + R-1/R-2)

**Files:**
- Create: `crates/autoagent-cli/tests/e2e_reversibility_matrix.rs`
- Create: `crates/autoagent-core/tests/policy_enforcement_matrix.rs`

**Step 1: Write the failing tests** — exhaustive matrices mapping to SPEC-1 §6 launch metrics:
```rust
// reversibility: every FileOperationKind round-trips to the original tree
#[test] fn every_op_kind_is_reversible() {
    for case in [OpCase::create(), OpCase::replace(), OpCase::append(),
                 OpCase::delete(), OpCase::rename(), OpCase::mkdir()] {
        let (root, run_id) = apply_case(&case);
        let before = snapshot_tree(&root);          // hash the whole tree pre-apply (captured in fixture)
        revert(&root, &run_id);
        assert_eq!(snapshot_tree(&root), case.original_tree_hash, "op {:?} not reversible", case.kind);
    }
}
// policy: every guard rule denies, 0 out-of-policy writes
#[test] fn no_out_of_policy_write_lands() {
    for bad in ["../escape","/etc/passwd",".git/config",".env","target/x","node_modules/y"] {
        assert!(apply_write_to(bad).is_err(), "{bad} should be refused");
        assert!(!path_was_written(bad));
    }
}
```

**Step 2: Run to verify it fails** → FAIL (any gap is a release blocker)

**Step 3: Make it pass** — fix any reversibility/policy gap surfaced. Per SPEC-1 §6.3 these are **stop-the-line**; a failing case blocks 1.0.0. No production shortcuts.

**Step 4: Run to verify it passes** → PASS — 100% reversibility, 0 out-of-policy writes.

**Step 5: Commit** → `git add -A && git commit -m "test(1.0): reversibility + policy enforcement matrices"`

---

### Task 4: Audit-trail completeness verification (success criteria)

**Files:**
- Create: `crates/autoagent-core/tests/audit_completeness.rs`

**Step 1: Write the failing test**
```rust
#[test] fn every_op_and_command_appears_in_events() {
    let (root, run_id) = run_multi_op_fixture();   // plan with N ops + M commands
    let events = read_events(&root, &run_id);
    let applied = events.iter().filter(|e| e["type"]=="operation_applied").count();
    let cmds = events.iter().filter(|e| e["type"]=="command_finished").count();
    assert_eq!(applied, EXPECTED_OPS);
    assert_eq!(cmds, EXPECTED_COMMANDS);
    assert!(seq_is_monotonic_without_gaps(&events));   // §3.4.3 integrity rule
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Make it pass** — close any event-emission gap so 100% of operations and commands are recorded with monotonic `seq` (SPEC-1 §2.2 audit completeness + §3.4.3 integrity).

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "test(1.0): audit-trail completeness verification"`

---

### Task 5: Performance benchmarks vs proposed NFR targets (OQ-1)

**Files:**
- Create: `crates/autoagent-core/benches/scan_and_apply.rs` (criterion or a simple timed harness)
- Create: `crates/autoagent-cli/tests/perf_smoke.rs`

**Step 1: Write the failing test** — a perf smoke gate against SPEC-1 §2.2 *proposed* targets (these targets are OQ-1; confirm or adjust the thresholds here):
```rust
#[test] fn scan_10k_files_under_target() {
    let root = generate_repo_with_files(10_000);
    let t = time(|| scan(&root, &["**/*".into()], &["target/**".into()]).unwrap());
    assert!(t.as_secs_f64() < 2.0, "scan p-target exceeded: {:?}", t);   // proposed p95 < 2s
}
```

**Step 2: Run to verify it fails (or passes)** → measure; if it fails, optimize the scanner (parallel walk) OR, with user sign-off on OQ-1, adjust the documented target. Do NOT silently relax it.

**Step 3: Make it pass** — optimize and/or record confirmed targets back into SPEC-1 §2.2 (removing the "proposed" label once confirmed).

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "test(1.0): performance smoke gate vs NFR targets"`

---

### Task 6: Documentation freeze (README, help, schemas, CHANGELOG)

**Files:**
- Create/Modify: `README.md` (install, the SPEC-1 Appendix B help, the supervised/approval defaults)
- Create: `CHANGELOG.md` (0.1.0 → 1.0.0 history)
- Create: `docs/schemas.md` (links the exported JSON Schemas + the events catalog)
- Verify: `autoagent --help` output matches SPEC-1 Appendix D

**Step 1: Write the failing test** — a doc-sync test:
```rust
#[test] fn readme_documents_all_commands() {
    let readme = include_str!("../../../README.md");
    for cmd in ["init","doctor","analyze","plan","apply","run","evolve","patch","revert","memory","config"] {
        assert!(readme.contains(cmd), "README missing command: {cmd}");
    }
    assert!(readme.contains("Write operations require approval"));  // the load-bearing default
}
```

**Step 2: Run to verify it fails** → FAIL

**Step 3: Write the docs** — README leading with the product identity ("controlled self-authoring, not uncontrolled self-replication"), install via `cargo install` and release binaries (resolve OQ-2 here), the full command table, and the default safety posture. All code blocks language-tagged; headings/emphasis correct.

**Step 4: Run to verify it passes** → PASS

**Step 5: Commit** → `git add -A && git commit -m "docs(1.0): README, CHANGELOG, schema docs, help parity"`

---

### Task 7: Release engineering (CI, versioning, binaries)

**Files:**
- Create: `.github/workflows/ci.yml` (fmt, clippy -D warnings, test --workspace, build --release on linux/macos/windows)
- Create: `.github/workflows/release.yml` (tag-triggered: build release binaries, attach to GitHub release)
- Modify: workspace `version` → `1.0.0`

**Step 1: Validate workflows locally**
```
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --release
```
→ Expected: all green; `target/release/autoagent` exists.

**Step 2: Bump version + tag**
- Set `[workspace.package].version = "1.0.0"`.
- `cargo build` to refresh the lockfile.

**Step 3: Commit + tag**
`git add -A && git commit -m "release: 1.0.0" && git tag v1.0.0`

---

### Task 8: Final milestone exit (1.0.0 gate)

**Step 1: Full gate** — fmt clean, clippy zero-warning, `cargo test --workspace` green (all unit + E2E + matrices), `cargo build --release` ok.

**Step 2: Verify M8 / 1.0.0 exit criteria (SPEC-1 §5 + §6):**
- Schemas stable + documented (Tasks 1, 2, 6) ✓
- Full reversibility verified — 100% (Task 3) ✓
- Policy enforcement verified — 0 out-of-policy writes (Task 3) ✓
- Audit logging complete — 100% (Task 4) ✓
- No breaking changes pending (freeze snapshots green) ✓

**Step 3: Verify the §14 implementation principles hold:**
- Mutation engine safe before planner powerful ✓ (build order M1→M3)
- Every mutation structured/validated/snapshotted/logged/reversible ✓ (Task 3, 4)
- Every command explicit/logged/validated ✓ (command guard + events)
- Self-authoring opt-in + supervised ✓ (M6 EvolveGuard)
- Plans are validated contracts ✓ (plan_validator)
- Types model the domain ✓
- CLI friendly, core independently testable ✓ (crate split)

**Step 4: Commit** → `git add -A && git commit -m "chore(1.0.0): stable release milestone exit — all criteria verified"`

---

## Open design questions (resolve during execution)
- **OQ-1** confirmation: lock the NFR/performance targets (Task 5) and strip "proposed" from SPEC-1 §2.2.
- **OQ-2:** finalize distribution channel (cargo install + release binaries assumed in Task 6/7).
- Post-1.0 deprecation policy: how schema v2 would be introduced without breaking v1 readers (PROPOSED: additive-only fields + `schema_version` gate already in Task 1).

---

## Cross-milestone note
This is the final plan in the set (M1–M8). Sequencing per SPEC-1 §5 dependency graph:
`M1 → M2 → M3 → M4 → M5`, then `M6 → M7 → M8`. M1 is the only plan derived purely by extraction; M2–M8 carry **PROPOSED DESIGN** labels and embed open design questions to confirm during `lore:execute`.
