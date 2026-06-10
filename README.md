# AutoAgent

> Describe a job. AutoAgent writes the code to get it done — and makes every change reviewable and reversible.

AutoAgent is a Rust-native local coding agent. You give it an objective in plain language; it reads your project, **authors the actual code** to accomplish it, applies the changes, runs your validation commands, and repairs itself if validation fails — then records a patch so any run can be rolled back with one command.

```bash
autoagent run "add an in-memory LRU cache to the request handler and a test for it"
```

That single command makes the agent write the new files and edits, apply them, run `cargo test`, and — if the tests fail — re-plan against the failure and try again, all under a policy gate that keeps it inside your workspace.

## The agent writes the code

The job-doing loop is the whole point:

1. **Understand** — AutoAgent scans the project (languages, structure, dependencies) and recalls past decisions from project memory.
2. **Author** — it asks an LLM to produce a structured *plan*: the precise list of files to create/modify and **the real content of every one of them**. The model authors the code; AutoAgent owns the contract format.
3. **Gate** — every proposed operation and command is validated against the policy engine *before anything touches disk*. A plan that tries to escape the workspace, write a blocked path, or read a secret is refused — not applied.
4. **Apply** — each touched file is snapshotted, then the changes are written for real.
5. **Validate** — your configured commands (`cargo test`, lint, build, …) run as real subprocesses; the output is captured into a report.
6. **Repair** — if validation fails, AutoAgent re-plans against the captured error and applies a fix, bounded by `max_steps_per_run`.
7. **Record** — a complete, append-only audit trail and a reversible patch land under `.agent/runs/<run-id>/`.

The model only ever *proposes*. The engine *applies validated, reversible contracts*. That separation is what makes letting an agent write your code safe.

## Quickstart

```bash
# 1. Build the CLI
cargo build --release            # produces target/release/autoagent

# 2. Initialize in your project
autoagent init                   # writes Autoagent.toml + the .agent/ workspace

# 3. Point it at a model (see "Bring your own model" below), then give it a job:
autoagent run "implement pagination on the /users endpoint and cover it with a test"
```

Prefer to review before anything runs? Generate the code as a plan first, read it, then apply it:

```bash
autoagent plan "implement pagination on the /users endpoint"   # writes .agent/plans/*.plan.json (+ .md)
autoagent run --from .agent/plans/<that-plan>.plan.json "implement pagination"
autoagent revert <run-id>        # full rollback of an applied run
```

## Bring your own model

AutoAgent does not ship a model — it directs one. Code authoring quality tracks the model you point it at, and your source stays on your machine unless you explicitly opt into cloud egress.

Add an `[llm]` block to `Autoagent.toml`:

```toml
[llm]
# Local by default — nothing leaves your machine. Runs against an Ollama-style
# /api/generate endpoint.
provider = "local"
model = "qwen3-coder:30b"          # any local model that supports /api/generate
endpoint = "http://localhost:11434"
code_egress_opt_in = false
```

To use a cloud model, you must **explicitly opt in** (this acknowledges that source code leaves the machine) and provide the key via the environment — never in config:

```toml
[llm]
provider = "anthropic"             # or "openai"
model = "claude-opus-4-8"
code_egress_opt_in = true          # required for any cloud provider
```

```bash
export ANTHROPIC_API_KEY=...       # or OPENAI_API_KEY for the openai provider
autoagent run "refactor the parser into its own module"
```

If a cloud provider is selected without `code_egress_opt_in = true`, AutoAgent refuses to run — your code cannot leave the machine by accident.

## Why it's safe to let it write code

These defaults are on out of the box and are the load-bearing posture:

- **Write operations require approval** (`require_approval_before_write = true`). Pass `--yes` to authorize a run up front.
- **Unknown commands require approval** (`require_approval_before_command = true`); validation runs only policy-allowed commands.
- **Every applied run is snapshotted, logged, and reversible** — `autoagent revert <run-id>` restores the prior state from the run's patch.
- **The workspace is a hard boundary.** Writes outside the configured allowed paths are denied; `.git`, `target`, `.env*`, SSH material, and any path that escapes the workspace are blocked by default — enforced against the *model's own output*, so a bad suggestion is rejected before it ever reaches disk.
- **Self-modification is off** (`allow_self_modification = false`). The agent can work on its *own* source via `evolve`, but only plan-only by default; self-apply is opt-in and runs on an isolated `autoagent/evolve/<run-id>` branch. The stance is **controlled self-authoring, not uncontrolled self-replication**.

## Commands

| Command | Purpose | Default write behavior |
| --- | --- | --- |
| `run` | **Give the agent a job**: plan → write the code → apply → validate → repair → report | Supervised; `--yes` to auto-approve |
| `plan` | Have the agent author the code as a reviewable plan (or import one with `--from`) | Writes plan files only |
| `apply` | Apply a structured plan through the mutation engine | Writes only approved planned changes |
| `evolve` | Let AutoAgent author changes to *its own* source | Plan-only unless `--apply` + `allow_self_modification` |
| `revert` | Roll back a previous run | Writes rollback changes |
| `analyze` | Scan the project and write a report | Writes the report only |
| `init` | Create `Autoagent.toml` + the `.agent/` workspace | Writes after confirmation / `--yes` |
| `doctor` | Check toolchain, config, commands, workspace health | Read-only |
| `patch` | List or show patch artifacts | Read-only |
| `config` | Show or validate `Autoagent.toml` | Read-only |
| `memory` | Show or manage project memory | Depends on subcommand |
| `tools` | List registered plugin tools | Read-only |

## Use it from your own code

The same agent is available as a typed SDK in three languages, each wrapping the native engine:

```python
# Python — pip install autoagent
from autoagent import AutoAgent
aa = AutoAgent("/path/to/repo")
outcome = aa.run_sync("add a healthcheck endpoint", approve=True)   # the agent writes + applies the code
print(outcome.run_id, outcome.final_state)
```

```ts
// Node/TypeScript — npm install @autoagent/sdk
import { AutoAgent } from "@autoagent/sdk";
const aa = new AutoAgent("/path/to/repo");
const outcome = await aa.run("add a healthcheck endpoint", null, true);
```

```ts
// Deno — import { AutoAgent } from "jsr:@autoagent/sdk";
const aa = new AutoAgent("/path/to/repo");
const outcome = aa.runSync("add a healthcheck endpoint", null, true);
```

See [`sdk/python`](sdk/python/README.md), [`sdk/node`](sdk/node/README.md), and [`sdk/deno`](sdk/deno/README.md). Mutating calls are fail-closed: without approval they raise/throw an `AutoAgentError` with a `policy.*` code rather than touching the tree.

## Plugins

Plugins extend AutoAgent through the `autoagent-plugin-sdk` (`Plugin`/`Tool`/`HostContext` traits). Native and sandboxed WASM (wasmtime) plugins are both supported, and **every plugin's I/O is routed through the same policy engine** — a plugin cannot perform any write or command the policy would reject.

## Workspace layout

```text
crates/
  autoagent-cli/         # thin user-facing CLI
  autoagent-core/        # the policy-controlled mutation engine (plan → apply → validate → repair → evolve)
  autoagent-plugin-sdk/  # stable plugin ABI
  autoagent-bingen/      # single-source binding generator (native Python/Node/Deno bindings)
sdk/
  python/ node/ deno/    # typed language SDKs over the native bindings
docs/
  specs/                 # SPEC-1
  plans/                 # milestone implementation plans
schemas/                 # frozen JSON Schemas (see docs/schemas.md)
```

## License

MIT — see [LICENSE](LICENSE).
