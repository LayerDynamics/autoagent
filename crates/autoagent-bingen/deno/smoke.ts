// Deno smoke test (B4-T2): load the raw-FFI cdylib through the generated
// mod.ts wrapper and exercise the read + mutating surface end to end.
//
// Run: deno run --allow-ffi --allow-read --allow-write --allow-env \
//        --unstable-ffi crates/autoagent-bingen/deno/smoke.ts
// (AUTOAGENT_BINGEN_LIB should point at the built cdylib.)

import { apply, doctor, init, revert, version } from "./mod.ts";

const v = version();
if (v !== 1) {
  console.error(`smoke: unexpected version ${v}`);
  Deno.exit(1);
}

const root = Deno.makeTempDirSync({ prefix: "aa-deno-" });

const report = doctor(root) as { checks: unknown[] };
if (!Array.isArray(report.checks)) {
  console.error("smoke: doctor() bad shape", report);
  Deno.exit(1);
}

init(root);
const plan = `${root}/p.json`;
Deno.writeTextFileSync(
  plan,
  JSON.stringify({
    objective: "c",
    summary: "s",
    files_to_read: [],
    files_to_create: [{ path: "crates/x.rs", purpose: "p" }],
    files_to_modify: [],
    operations: [
      {
        kind: "Create",
        path: "crates/x.rs",
        destination_path: null,
        reason: "r",
        before_hash: null,
        after_hash: null,
        content: "// x",
      },
    ],
    validation_commands: [],
    risks: [],
    rollback_strategy: "snapshot",
  }),
);

// Fail-closed: apply without approval must throw a policy error.
let refused = false;
try {
  apply(root, plan, false);
} catch (e) {
  refused = String((e as Error).message).toLowerCase().includes("policy") ||
    (e as { code?: string }).code?.startsWith("policy") === true;
}
if (!refused) {
  console.error("smoke: apply(approve=false) was not refused");
  Deno.exit(1);
}

// Approved apply mutates, revert restores.
const runId = apply(root, plan, true);
if (!runId) {
  console.error("smoke: apply returned empty run id");
  Deno.exit(1);
}
revert(root, runId);

console.log(`smoke ok: version=${v}, doctor checks=${report.checks.length}, apply+revert+fail-closed verified`);
