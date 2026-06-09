// Fail-closed approval through the real napi backend (FR-7 / FR-20): a
// privileged op with approve=false must throw a policy error and not mutate.
//
// Run after building the cdylib: `cargo build -p autoagent-bingen
// --features node-napi --release --lib`, then `node --test __test__/`.

import test from "node:test";
import assert from "node:assert";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
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
  const root = mkdtempSync(join(tmpdir(), "aa-"));
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

test("apply without approval throws a policy error and does not mutate", () => {
  const { root, plan } = seed();
  assert.throws(
    () => addon.apply(root, plan, false),
    /policy/i,
    "apply(approve=false) must throw a policy error"
  );
  assert.ok(!existsSync(join(root, "crates/x.rs")), "workspace must be unchanged");
});

test("apply with approval succeeds and creates the file", () => {
  const { root, plan } = seed();
  const runId = addon.apply(root, plan, true);
  assert.ok(runId, "apply must return a run id");
  assert.ok(existsSync(join(root, "crates/x.rs")), "file must be created");
});
