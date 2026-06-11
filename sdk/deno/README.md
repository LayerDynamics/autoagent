# AutoAgent Deno SDK

Typed Deno bindings for [AutoAgent](../../README.md) — a Rust-native local agent
for safe, reversible, policy-controlled codebase mutation.

`@layerdynamics/autoagent-sdk` wraps the native FFI binding with typed models
(`DoctorReport`, `RunOutcome`, `ProjectAnalysis`, …), an `AutoAgentError` class,
and an `AutoAgent` client. Mutating operations are **fail-closed**: an
unapproved op throws rather than touching the tree.

## Install

```ts
import { AutoAgent, doctor } from "jsr:@layerdynamics/autoagent-sdk";
```

The binding loads a compiled cdylib via Deno FFI. Build it once and point
`AUTOAGENT_BINGEN_LIB` at it:

```bash
cargo build --release -p autoagent-bingen --features deno-ffi
export AUTOAGENT_BINGEN_LIB="$PWD/target/release/libautoagent_bingen.dylib"  # .so on Linux
```

FFI requires explicit permissions: `--allow-ffi --allow-read --allow-write --allow-env`.

## Quick start

```ts
// run: deno run --allow-ffi --allow-read --allow-write --allow-env example.ts
import { doctor, runSync, revert, version } from "jsr:@layerdynamics/autoagent-sdk";

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
import { AutoAgent } from "jsr:@layerdynamics/autoagent-sdk";

const aa = new AutoAgent("/path/to/your/repo");
aa.doctor();                       // -> DoctorReport
aa.analyze();                      // -> ProjectAnalysis
const runId = aa.apply("plan.json", true);
aa.revert(runId);
```

> The FFI surface exposes the synchronous `runSync`/`evolveSync`. Async
> (`non_blocking`) run/evolve are available through the native deno_bindgen path.

## Safety

Mutating operations (`apply`, `runSync`, `evolveSync`) are fail-closed: without
the approval flag they throw `AutoAgentError` with a `policy.*` code.

```ts
import { AutoAgent, AutoAgentError } from "jsr:@layerdynamics/autoagent-sdk";

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
`patchShow`, `memoryShow`, `toolsList`, `init`, `apply`, `revert`, `runSync`,
`evolveSync`.

Client: `new AutoAgent(root)` exposes the same surface as methods (root bound
once).

The typed models in `_models.ts` are generated from the core Rust structs
(`schemars`) and drift-guarded — see [the SDK plan](../../docs/plans/2026-06-10-autoagent-sdks.md).

## Test

```bash
deno test --allow-ffi --allow-read --allow-write --allow-env
```
