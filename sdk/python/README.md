# AutoAgent Python SDK

Typed Python bindings for [AutoAgent](../../README.md) — a Rust-native local agent
for safe, reversible, policy-controlled codebase mutation.

The native pyo3 extension (`autoagent._native`) returns JSON strings; this SDK
parses them into typed dataclasses (`DoctorReport`, `RunOutcome`,
`ProjectAnalysis`, …), maps native errors to `AutoAgentError`, and preserves the
engine's **fail-closed** safety: an unapproved mutating op raises, it does not
apply.

## Install

```bash
# from a published wheel (import name stays `autoagent`)
pip install autoagent-sdk

# from source (builds the bundled native extension)
cd sdk/python
maturin develop --release
```

The wheel is a mixed maturin layout: the compiled `_native` extension ships
alongside the pure-Python typed wrapper. No separate native package is required.

## Quick start

```python
import autoagent

root = "/path/to/your/repo"

# 1. Health check — returns a typed DoctorReport, not a JSON string.
report = autoagent.doctor(root)
print(f"schema v{autoagent.version()}")
for check in report.checks:
    print(f"  {'ok ' if check.ok else 'FAIL'} {check.name}: {check.detail}")

# 2. Run an objective with approval (applies changes; reversible).
outcome = autoagent.run_sync(root, "add a CHANGELOG entry", approve=True)
print(f"run {outcome.run_id}: {outcome.final_state}")

# 3. Roll it back.
autoagent.revert(root, outcome.run_id)
```

### Client class

```python
from autoagent import AutoAgent

aa = AutoAgent("/path/to/your/repo")
aa.doctor()                       # -> DoctorReport
aa.analyze()                      # -> ProjectAnalysis
run_id = aa.apply("plan.json", approve=True)
aa.revert(run_id)
```

### Async

`run` and `evolve` are `async` (the work runs off the event loop):

```python
import asyncio
from autoagent import AutoAgent

async def main():
    aa = AutoAgent("/path/to/your/repo")
    outcome = await aa.run("refactor the parser", approve=True)
    print(outcome.run_id)

asyncio.run(main())
```

## Safety

Mutating operations (`apply`, `run`, `evolve`) are fail-closed: without
`approve=True` they raise `AutoAgentError` with a `policy.*` code rather than
touching the tree.

```python
from autoagent import AutoAgent, AutoAgentError

try:
    AutoAgent(root).apply("plan.json")  # approve defaults to False
except AutoAgentError as e:
    print(e.code)       # e.g. "policy.approval_required"
    print(e.exit_code)  # numeric exit code from core's taxonomy
```

## API

Functional: `version`, `doctor`, `analyze`, `config_show`, `patch_list`,
`patch_show`, `memory_show`, `tools_list`, `init`, `apply`, `revert`,
`run_sync`/`run`, `evolve_sync`/`evolve`.

Client: `AutoAgent(root)` exposes the same surface as methods (root bound once).

The typed models in `autoagent._models` are generated from the core Rust structs
(`schemars`) and drift-guarded — see [the SDK plan](../../docs/plans/2026-06-10-autoagent-sdks.md).
