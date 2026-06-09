# SPEC-1 Conformance Map

Every functional requirement from [SPEC-1 §2.1](specs/SPEC-1-autoagent.md) mapped
to the implementing module and the test that verifies it. Run `cargo test
--workspace` to execute all of them.

| FR | Pri | Implementation | Verifying test |
|----|-----|----------------|----------------|
| FR-1 | MUST | `autoagent-cli/src/main.rs` — all 11 subcommands (+ `tools`) | `main::tests::parses_*`; every `tests/e2e_*` |
| FR-2 | MUST | `runtime/init.rs` `init_workspace` (all 7 `.agent/` dirs) | `init::tests::init_writes_config_and_dirs` |
| FR-3 | MUST | `runtime/doctor.rs` — rust/cargo/git/config/workspace/agent_dir + per-`[commands]` availability | `doctor::tests::*` |
| FR-4 | MUST | `config/config_schema.rs` `load`; `agent_loop::apply` aborts on missing config | `config_loader::tests::missing_file_is_config_error` |
| FR-5 | MUST | `analysis/file_scanner.rs` — include/exclude globs + workspace `.gitignore` | `file_scanner::tests::*` |
| FR-6 | MUST | `agent_loop::apply` → `plan_validator::validate_plan` before any write | `reversibility_and_policy_matrix::no_out_of_policy_write_lands` |
| FR-7 | MUST | `editing/snapshot_manager.rs`; `agent_loop` snapshots before mutation | `reversibility_and_policy_matrix::*_is_reversible` |
| FR-8 | MUST | `editing/file_operation.rs` (7 kinds); `file_editor.rs` | matrix: all 7 op-kind reversibility tests |
| FR-9 | MUST | `agent_loop::apply` writes the full run folder; `logging/run_logger.rs` | `run_folder_contract::apply_produces_complete_run_folder_and_workspace_log` |
| FR-10 | MUST | `logging/event_log.rs` `with_workspace_log` mirror | `run_folder_contract` (workspace log assertions) |
| FR-11 | MUST | `validation/command_runner.rs` → `ValidationReport` | `command_runner::tests::*` |
| FR-12 | MUST | `runtime/revert.rs` (snapshot restore + drift detection) | matrix reversibility; `revert::tests::*` |
| FR-13 | MUST | `safety/command_guard.rs` | `command_guard::tests::*` (+ proptest) |
| FR-14 | MUST | `safety/path_guard.rs` (escape>symlink>block>allow) | `path_guard::tests::*` (+ proptest) |
| FR-15 | MUST | `agent_loop::apply` write gate; `commands::run` command gate | `agent_loop::apply_without_approval_is_refused`; `e2e_apply_revert` |
| FR-16 | MUST | `cli commands::{patch_list, patch_show}` | exercised via CLI; `e2e_apply_revert` writes patches |
| FR-17 | MUST | `cli commands::config_show` (load = validate) | `config_loader::tests` (validation path) |
| FR-18 | SHOULD | `analysis::{project_analyzer, dependency_analyzer, report_writer}` | `e2e_analyze`; `project_analyzer::tests::analyze_produces_counts` |
| FR-19 | SHOULD | `planning/plan_writer.rs`; `cli commands::plan` | `e2e_plan`; `plan_writer::tests::writes_paired_json_and_md` |
| FR-20 | SHOULD | `runtime/run_workflow.rs` | `e2e_run`; `run_workflow::tests::run_applies_and_validates_clean` |
| FR-21 | SHOULD | `memory/*`; `cli commands::memory_*` (all 5 entry types) | `e2e_memory`; `memory_store::tests::*` |
| FR-22 | SHOULD | `planning/llm/*` (local + Anthropic + OpenAI), `redactor`, opt-in gate | `llm::*::tests`; `planner::tests::*` |
| FR-23 | COULD | `runtime/{evolve_guard, evolve_workflow}.rs`; `git/branch_manager.rs` | `e2e_evolve`; `evolve_workflow::tests::*` |
| FR-24 | COULD | `autoagent-plugin-sdk`; `plugins/{host_context, registry, wasm_host}.rs` | `plugins::*::tests`; `wasm_host::tests::wasm_host_write_to_git_is_denied_by_policy` |
| FR-25 | COULD | `runtime/{repair, run_workflow}.rs` (bounded by `max_steps_per_run`) | `run_workflow::tests::run_repairs_after_failing_validation` |
| FR-26 | WON'T | Enforced: path guard workspace-boundary; no replication/persistence paths exist | `path_guard` escape tests; matrix |
| FR-27 | WON'T | No remote/push/deploy code path; `git_client` is read-only | `git_client.rs` (status/diff/branch only) |
| FR-28 | WON'T | `command_guard` + `path_guard` gate every command/write | `command_guard`/`path_guard` tests; matrix |

## Non-functional verification

- **Reversibility 100%** — `tests/reversibility_and_policy_matrix.rs` (all 7 op kinds).
- **Zero out-of-policy writes** — same file, `no_out_of_policy_write_lands`.
- **Audit completeness** — `tests/audit_completeness.rs` (monotonic gap-free seq).
- **Performance** — `tests/perf_smoke.rs` (4k files in ~22ms; OQ-1 target met).
- **Schema freeze** — `planning::plan_schema::tests::schema_matches_frozen_golden`.

## Honest boundaries

- **M2–M8 implement designs the spec delegates to the implementer.** The §13
  roadmap names deliverables, not internal schemas/algorithms; those concrete
  choices (detection heuristics, memory schemas, the LLM provider trait, the
  WASM ABI) fulfill the named deliverables and are not spec deviations.
- **Cloud LLM providers are verified at the request/contract level** (headers,
  body, opt-in egress gate). No live Anthropic/OpenAI call is made in tests; the
  local provider is tested against an in-process HTTP server.
- **Open questions remain open**: OQ-4 (exact redaction policy) and OQ-5 (which
  providers ship first). OQ-1 (NFR targets) is now measured and met.
