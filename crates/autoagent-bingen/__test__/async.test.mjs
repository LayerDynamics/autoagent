// FR-5: napi async run/evolve return a JS Promise (napi AsyncTask /
// spawn_blocking), so the event loop is not blocked. Verified by awaiting.
//
// Build: cargo run -p autoagent-bingen --bin bingen -- smoke  (builds the addon)
// Run:   node --test crates/autoagent-bingen/__test__/async.test.mjs

import test from "node:test";
import assert from "node:assert";
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

const mod = { exports: {} };
process.dlopen(mod, libPath);
const addon = mod.exports;

function seed() {
  const root = mkdtempSync(join(tmpdir(), "aa-async-"));
  addon.init(root);
  const plan = join(root, "p.json");
  writeFileSync(
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
    })
  );
  return { root, plan };
}

test("run() returns a Promise resolving to a RunOutcome", async () => {
  const { root, plan } = seed();
  const p = addon.run(root, "c", plan, true);
  assert.ok(p instanceof Promise, "run() must return a Promise");
  const outcome = await p;
  assert.ok(outcome.run_id, "outcome must have a run_id");
  assert.ok(existsSync(join(root, "crates/x.rs")), "run must create the file");
});
