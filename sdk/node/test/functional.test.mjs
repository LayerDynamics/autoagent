// Typed functional API (S3-T2): native objects -> typed results; native errors
// -> AutoAgentError. Imports the built dist/.
import test from "node:test";
import assert from "node:assert";
import { mkdtempSync, writeFileSync } from "node:fs";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import * as aa from "../dist/index.js";

function seed() {
  const root = mkdtempSync(join(tmpdir(), "aa-sdk-"));
  aa.init(root);
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
  return { root, plan };
}

test("version is a number", () => {
  assert.strictEqual(aa.version(), 1);
});

test("doctor returns a typed report with checks[]", () => {
  const { root } = seed();
  const report = aa.doctor(root);
  assert.ok(Array.isArray(report.checks));
  assert.ok(typeof report.checks[0].name === "string");
});

test("apply without approval throws AutoAgentError (policy)", () => {
  const { root, plan } = seed();
  assert.throws(
    () => aa.apply(root, plan, false),
    (e) => e instanceof aa.AutoAgentError && e.code.startsWith("policy"),
  );
  assert.ok(!existsSync(join(root, "crates/x.rs")));
});

test("apply + revert roundtrip", () => {
  const { root, plan } = seed();
  const runId = aa.apply(root, plan, true);
  assert.ok(runId);
  assert.ok(existsSync(join(root, "crates/x.rs")));
  aa.revert(root, runId);
  assert.ok(!existsSync(join(root, "crates/x.rs")));
});

test("async run returns a typed RunOutcome", async () => {
  const { root, plan } = seed();
  const outcome = await aa.run(root, "c", plan, true);
  assert.ok(outcome.run_id);
  assert.ok("final_state" in outcome);
});
