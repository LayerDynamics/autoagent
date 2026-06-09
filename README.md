# AutoAgent

> A Rust-native local agent runtime for safe, reversible, policy-controlled codebase evolution.

AutoAgent is a developer agent that can **change** a codebase — inspect it, plan edits, apply them, validate, and revert — while guaranteeing that every change is bounded, reviewable, and reversible. Its defining stance is **controlled self-authoring, not uncontrolled self-replication**: AutoAgent can eventually work on its *own* source tree, but only as a supervised workflow gated by policies, snapshots, patches, validation, and rollback. It never persists covertly, spreads across machines, or propagates without supervision.

The architectural center is a **safe mutation engine**: file mutation and command execution are privileged operations that must pass a policy engine. Planning intelligence (including LLM-assisted planning) is layered on top only after the engine can already read structured plans, apply changes, validate, and revert. The model *proposes* plans; the engine *applies validated contracts*.

## Safety defaults

These defaults are the load-bearing posture and are on out of the box:

- **Write operations require approval** (`require_approval_before_write = true`).
- **Unknown commands require approval** (`require_approval_before_command = true`).
- **Self-modification is off** (`allow_self_modification = false`); `evolve` is plan-only unless explicitly applied, and self-apply runs on an isolated `autoagent/evolve/<run-id>` branch.
- **Every applied run is snapshotted, logged, and reversible.** Nothing is written outside the configured allowed paths; `.git`, `target`, `.env*`, SSH material, and paths outside the workspace are denied by default.

## Install

```bash
# from source
cargo install --path crates/autoagent-cli

# or build the workspace
cargo build --release   # produces target/release/autoagent
```

Tagged releases also publish prebuilt binaries on GitHub.

## Commands

| Command | Purpose | Default write behavior |
| --- | --- | --- |
| `init` | Create `Autoagent.toml` + the `.agent/` workspace | Writes after confirmation / `--yes` |
| `doctor` | Check toolchain, config, commands, workspace health | Read-only |
| `analyze` | Scan the project and write a project report | Writes the report only |
| `plan` | Create or import a structured implementation plan | Writes plan files only |
| `apply` | Apply a structured plan through the mutation engine | Writes only approved planned changes |
| `run` | Plan, apply, validate, and report in a supervised workflow | Supervised by default |
| `evolve` | Create a controlled self-authoring plan for AutoAgent itself | Plan-only by default |
| `patch` | List or show patch artifacts | Read-only unless saving |
| `revert` | Revert a previous AutoAgent run | Writes rollback changes |
| `memory` | Show or manage project memory | Depends on subcommand |
| `config` | Show or validate `Autoagent.toml` | Read-only |
| `tools` | List registered plugin tools | Read-only |

### Example

```bash
autoagent init
autoagent analyze
autoagent apply .agent/plans/add-cache.plan.json   # prompts for approval
autoagent revert 20260609T120000Z-add-cache        # full rollback
```

## How it works

1. Load `Autoagent.toml` and validate the workspace boundary.
2. Read or generate a structured plan (the contract).
3. Validate every operation and command against the policy engine.
4. Snapshot every touched file, then apply operations.
5. Run validation commands; build a report.
6. Write a complete, append-only audit trail under `.agent/runs/<run-id>/`.

Any run is fully reconstructable from its run folder, and any applied run is reversible with `autoagent revert <run-id>`.

## Plugins

Plugins extend AutoAgent through the `autoagent-plugin-sdk` (`Plugin`/`Tool`/`HostContext` traits). Native and sandboxed WASM (wasmtime) plugins are both supported, and **every plugin's I/O is routed through the same policy engine** — a plugin cannot perform any write or command the policy would reject.

## Workspace layout

```text
crates/
  autoagent-cli/         # thin user-facing CLI
  autoagent-core/        # the policy-controlled mutation engine
  autoagent-plugin-sdk/  # stable plugin ABI
docs/
  specs/                 # SPEC-1
  plans/                 # milestone implementation plans
schemas/                 # frozen JSON Schemas (see docs/schemas.md)
```

## License

MIT — see [LICENSE](LICENSE).
