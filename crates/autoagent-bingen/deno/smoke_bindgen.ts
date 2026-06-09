// Deno smoke test for the PRIMARY deno_bindgen path: imports the
// deno_bindgen-generated bindings and exercises the read + mutating surface,
// including fail-closed approval.
//
// Prereqs:
//   cargo build -p autoagent-bingen --features deno-bindgen --lib
//   deno run -A deno/gen.ts <bindings.json> ../../../target/debug
// Run:
//   deno run --allow-ffi --allow-read --allow-write deno/smoke_bindgen.ts

import {
  aa_apply,
  aa_doctor,
  aa_init,
  aa_revert,
  aa_version,
} from "../bindings/bindings.ts";

/** Decode the status-tagged payload: `0`+data = ok, `1`+json = error. */
function unwrap(tagged: string): string {
  const tag = tagged.charAt(0);
  const body = tagged.slice(1);
  if (tag === "1") {
    const e = JSON.parse(body);
    throw new Error(`[${e.code}|${e.exit_code}] ${e.message}`);
  }
  return body;
}

const v = Number(unwrap(aa_version()));
if (v !== 1) {
  console.error(`smoke: unexpected version ${v}`);
  Deno.exit(1);
}

const root = Deno.makeTempDirSync({ prefix: "aa-denobindgen-" });
const report = JSON.parse(unwrap(aa_doctor(root))) as { checks: unknown[] };
if (!Array.isArray(report.checks)) {
  console.error("smoke: doctor bad shape", report);
  Deno.exit(1);
}

unwrap(aa_init(root));
const plan = `${root}/p.json`;
Deno.writeTextFileSync(
  plan,
  JSON.stringify({
    objective: "c",
    summary: "s",
    files_to_read: [],
    files_to_create: [{ path: "crates/x.rs", purpose: "p" }],
    files_to_modify: [],
    operations: [{
      kind: "Create",
      path: "crates/x.rs",
      destination_path: null,
      reason: "r",
      before_hash: null,
      after_hash: null,
      content: "// x",
    }],
    validation_commands: [],
    risks: [],
    rollback_strategy: "snapshot",
  }),
);

// Fail-closed: approve=0 must throw a policy error.
let refused = false;
try {
  unwrap(aa_apply(root, plan, 0));
} catch (e) {
  refused = String((e as Error).message).toLowerCase().includes("policy");
}
if (!refused) {
  console.error("smoke: apply(approve=0) was not refused");
  Deno.exit(1);
}

// Approve=1 mutates; revert restores.
const runId = unwrap(aa_apply(root, plan, 1));
if (!runId) {
  console.error("smoke: apply returned empty run id");
  Deno.exit(1);
}
unwrap(aa_revert(root, runId));

console.log(
  `smoke ok (deno_bindgen): version=${v}, doctor checks=${report.checks.length}, apply+revert+fail-closed verified`,
);
