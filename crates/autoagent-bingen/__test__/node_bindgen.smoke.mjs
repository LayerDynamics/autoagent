// Smoke test for the node-bindgen backend (Node alternative).
//
// node-bindgen's functions return the status-tagged string (`0`+payload /
// `1`+error JSON), like the FFI/deno_bindgen paths. The cdylib MUST be built
// with `-C link-dead-code` (RUSTFLAGS) so node-bindgen's registration
// constructors survive the cdylib `-dead_strip`; the generated backend also
// exports a `napi_register_module_v1` shim that Node 18+ looks up on load.
//
// Build: RUSTFLAGS="-C link-dead-code" cargo build -p autoagent-bingen \
//          --features node-bindgen --release --lib
// Run:   node crates/autoagent-bingen/__test__/node_bindgen.smoke.mjs

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { existsSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const targetDir = join(here, "..", "..", "..", "target", "release");
const ext =
  process.platform === "darwin" ? "dylib" : process.platform === "win32" ? "dll" : "so";
const prefix = process.platform === "win32" ? "" : "lib";
const libPath = join(targetDir, `${prefix}autoagent_bingen.${ext}`);

if (!existsSync(libPath)) {
  console.error(`smoke: cdylib not found at ${libPath}`);
  process.exit(1);
}

const mod = { exports: {} };
process.dlopen(mod, libPath);
const addon = mod.exports;

/** Decode the status-tagged payload. */
function unwrap(tagged) {
  const s = String(tagged);
  const tag = s.charAt(0);
  const body = s.slice(1);
  if (tag === "1") {
    const e = JSON.parse(body);
    const err = new Error(`[${e.code}|${e.exit_code}] ${e.message}`);
    err.code = e.code;
    throw err;
  }
  return body;
}

const v = Number(unwrap(addon.version()));
if (v !== 1) {
  console.error(`smoke: unexpected version ${v}`);
  process.exit(1);
}

const root = mkdtempSync(join(tmpdir(), "aa-nb-"));
const report = JSON.parse(unwrap(addon.doctor(root)));
if (!Array.isArray(report.checks)) {
  console.error("smoke: doctor bad shape", report);
  process.exit(1);
}

unwrap(addon.init(root));
const plan = join(root, "p.json");
writeFileSync(
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

// Fail-closed: approve=false must throw a policy error.
let refused = false;
try {
  unwrap(addon.apply(root, plan, false));
} catch (e) {
  refused = String(e.message).toLowerCase().includes("policy");
}
if (!refused) {
  console.error("smoke: apply(approve=false) was not refused");
  process.exit(1);
}

const runId = unwrap(addon.apply(root, plan, true));
unwrap(addon.revert(root, runId));

console.log(
  `smoke ok (node-bindgen): version=${v}, doctor checks=${report.checks.length}, apply+revert+fail-closed verified`,
);
