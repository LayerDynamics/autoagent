// AutoAgent client class (S3-T3).
import test from "node:test";
import assert from "node:assert";
import { mkdtempSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { AutoAgent, AutoAgentError } from "../dist/index.js";

function seed() {
  const root = mkdtempSync(join(tmpdir(), "aa-client-"));
  new AutoAgent(root).init();
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

test("client.doctor returns a typed report", () => {
  const { root } = seed();
  const aa = new AutoAgent(root);
  assert.ok(Array.isArray(aa.doctor().checks));
});

test("client.apply fail-closed without approval", () => {
  const { root, plan } = seed();
  const aa = new AutoAgent(root);
  assert.throws(
    () => aa.apply(plan, false),
    (e) => e instanceof AutoAgentError && e.code.startsWith("policy"),
  );
});

test("client async run", async () => {
  const { root, plan } = seed();
  const aa = new AutoAgent(root);
  const outcome = await aa.run("c", plan, true);
  assert.ok(outcome.run_id);
  assert.ok(existsSync(join(root, "crates/x.rs")));
});
