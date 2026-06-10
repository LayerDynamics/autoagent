# AutoAgent Node/TypeScript SDK

Typed Node.js bindings for [AutoAgent](../../README.md) — a Rust-native local
agent for safe, reversible, policy-controlled codebase mutation.

`@autoagent/sdk` wraps the native N-API binding (`@autoagent/native`) with typed
models (`DoctorReport`, `RunOutcome`, `ProjectAnalysis`, …), an `AutoAgentError`
class, and an `AutoAgent` client. Mutating operations are **fail-closed**: an
unapproved op throws rather than touching the tree.

## Install

```bash
npm install @autoagent/sdk
```

`@autoagent/native` (the compiled N-API addon) is a dependency and installs
automatically. The SDK ships full TypeScript types.

## Quick start

```ts
import { doctor, runSync, revert, version, AutoAgentError } from "@autoagent/sdk";

const root = "/path/to/your/repo";

// 1. Health check — returns a typed DoctorReport.
const report = doctor(root);
console.log(`schema v${version()}`);
for (const check of report.checks) {
  console.log(`  ${check.ok ? "ok  " : "FAIL"} ${check.name}: ${check.detail}`);
}

// 2. Run an objective with approval (applies changes; reversible).
const outcome = runSync(root, "add a CHANGELOG entry", null, true);
console.log(`run ${outcome.run_id}: ${outcome.final_state}`);

// 3. Roll it back.
revert(root, outcome.run_id);
```

### Client class

```ts
import { AutoAgent } from "@autoagent/sdk";

const aa = new AutoAgent("/path/to/your/repo");
aa.doctor();                       // -> DoctorReport
aa.analyze();                      // -> ProjectAnalysis
const runId = aa.apply("plan.json", true);
aa.revert(runId);
```

### Async

`run` and `evolve` return promises (work runs off the main thread):

```ts
import { AutoAgent } from "@autoagent/sdk";

const aa = new AutoAgent("/path/to/your/repo");
const outcome = await aa.run("refactor the parser", null, true);
console.log(outcome.run_id);
```

## Safety

Mutating operations (`apply`, `runSync`/`run`, `evolveSync`/`evolve`) are
fail-closed: without the approval flag they throw `AutoAgentError` with a
`policy.*` code.

```ts
import { AutoAgent, AutoAgentError } from "@autoagent/sdk";

try {
  new AutoAgent(root).apply("plan.json"); // approve defaults to false
} catch (e) {
  if (e instanceof AutoAgentError) {
    console.log(e.code);     // e.g. "policy.approval_required"
    console.log(e.exitCode); // numeric exit code from core's taxonomy
  }
}
```

## API

Functional: `version`, `doctor`, `analyze`, `configShow`, `patchList`,
`patchShow`, `memoryShow`, `toolsList`, `init`, `apply`, `revert`,
`runSync`/`run`, `evolveSync`/`evolve`.

Client: `new AutoAgent(root)` exposes the same surface as methods (root bound
once).

The typed models in `_models` are generated from the core Rust structs
(`schemars`) and drift-guarded — see [the SDK plan](../../docs/plans/2026-06-10-autoagent-sdks.md).
