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
model = "qwen2.5-coder:14b"         # a capable coder model that supports /api/generate
endpoint = "http://localhost:11434"
code_egress_opt_in = false
```

Authoring quality (and how often a self-edit succeeds first try) tracks the model.
`qwen2.5-coder:14b` is a fast, reliable default; **`qwen2.5-coder:32b`** is the most
reliable local option if you have the VRAM. The model must expose Ollama's
`/api/generate` (its capabilities include `completion`) — some chat-only models do
not. To check: `curl -s localhost:11434/api/show -d '{"model":"<name>"}' | grep completion`.

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

## Improving itself

Pointed at its own repository, AutoAgent is explicitly directed to implement changes to its *own* code when the objective calls for it. The `evolve` path uses a distinct **self-authoring** prompt that tells the model the workspace *is* the AutoAgent codebase (the Rust crates under `crates/`), to author the concrete changes to the relevant crate(s) — not a timid or empty plan — and to **always include the `cargo test` / `cargo clippy` / `cargo fmt` validation commands** so the supervised loop verifies (and, on failure, repairs) every self-change before it is accepted. A bug-fix objective is told to add a regression test.

```bash
# plan-only by default — review what it proposes to change about itself
autoagent evolve "make the local provider surface Ollama's error message on failure"

# opt in to let it apply the self-change (isolated on an autoagent/evolve/<id> branch)
#   set allow_self_modification = true in Autoagent.toml, then:
autoagent evolve --apply "…"
```

The same "implement the change when appropriate" directive is applied to ordinary `run`/`plan` jobs too: when an objective requires code, the agent is told to author the concrete operations and validation rather than describe them.

### How a run converges (and why it rarely ships a bad edit)

The supervised `run` loop is built to land a *valid* change, not just any change:

- **Navigates before it plans (agentic loop).** With a tool-capable model, the agent can call read-only tools — `read_file`, `grep`, `list_dir`, `run_command` — to inspect the real repository before proposing its plan, instead of guessing from metadata. Every tool is workspace-confined, secret-filtered, and (for `run_command`) policy-gated; the tools grant no write authority. Providers without tool support transparently fall back to schema-constrained one-shot planning.
- **Sees the files it edits, and edits surgically.** Before planning, the agent is shown the current contents of the files it intends to modify (bounded and secret-scrubbed). To change an existing file it prefers a `Substitute` op — an exact, unique anchor → replacement — which edits in place and *cannot* truncate or overwrite the rest of the file the way a full-file replace can.
- **Deterministic auto-heal.** If validation fails only on mechanical issues, the trusted toolchain fixes them with no model round-trip — `cargo fmt` for formatting, `cargo clippy --fix` for autofixable lints — then re-validates. The run's hashes are refreshed so the change stays revertible.
- **Iterative repair.** If a real failure remains, the failed attempt is reverted and the model is re-prompted with the **full** validation output *and its own previous code*, so it makes a targeted fix — bounded by `max_steps_per_run` (default 12).
- **Fails closed.** A run is only `Completed` if validation passes; otherwise it ends `Failed` with the tree restored. It does not ship a broken self-change.

This raises the first-try and within-budget success rate substantially, but success on any given objective still depends on the model — a weak model on a hard task can exhaust the repair budget. Point it at a stronger model (`qwen2.5-coder:32b` or a cloud model) for the highest reliability.

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
