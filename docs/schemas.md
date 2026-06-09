# AutoAgent On-Disk Schemas

All on-disk contracts are frozen at `schema_version = 1` as of 1.0.0. A binary
refuses any artifact declaring a newer schema version. Changes after 1.0.0 are
additive-only; a breaking change requires a `schema_version` bump and a
deliberate update to the golden files (guarded by a snapshot test).

## Plan (`*.plan.json`)

The machine contract consumed by `apply` and `run`. Canonical JSON Schema:
[`schemas/plan.schema.json`](../schemas/plan.schema.json) — exported by
`autoagent_core::planning::plan_schema::plan_json_schema()` and snapshot-locked
in `plan_schema::tests::schema_matches_frozen_golden`.

Key invariants:

- `operations` has at least one entry; each `kind` is one of
  `Create | Write | Replace | Append | Delete | Rename | CreateDirectory`.
- `rollback_strategy` MUST be `"snapshot"` (the only supported strategy).
- Content-bearing kinds (`Create`/`Write`/`Replace`/`Append`) require `content`;
  `Rename` requires `destination_path`.
- Every path is validated against the policy engine before any write.

## Run record (`run.json`)

Written per run under `.agent/runs/<run-id>/run.json`. Fields: `schema_version`,
`run_id`, `task_id`, `objective`, `mode`, `self_modification`, `state`,
`started_at`, `ended_at`, `duration_ms`, `plan_path`, `files_read`,
`files_modified[]`, `commands_executed[]`, `validation_passed`, `patch_path`,
`approvals[]`, `reverted_at`.

## Event log (`events.jsonl`)

Append-only, one JSON object per line. Common envelope: `schema_version`, `ts`,
`run_id`, `seq` (monotonic, gap-free per run), `type`, `state`, `data`. The
`type` catalog is defined in
`autoagent_core::logging::event_log::event_types` and includes `run_started`,
`plan_loaded`, `plan_rejected`, `snapshot_created`, `operation_applied`,
`command_started`/`command_finished`, `validation_completed`,
`run_completed`/`run_failed`, `revert_started`/`revert_completed`,
`drift_detected`, and `llm_request`.

## Configuration (`Autoagent.toml`)

The authoritative schema is `autoagent_core::config::config_schema` with the
canonical default in `default_config::default_toml()`. The optional `[llm]`
block (provider, model, endpoint, `code_egress_opt_in`) is additive and absent
in 0.1/0.2 configs.

## Error exit codes

`AutoAgentError::exit_code()` is a stable scripting contract: config/workspace=2,
plan=3, policy=4, editing=5, validation=6, revert=7, memory=8, and io/serde/
analysis/llm/plugin=1. Machine `error_code()` strings (e.g. `policy.blocked_path`)
are stable.
