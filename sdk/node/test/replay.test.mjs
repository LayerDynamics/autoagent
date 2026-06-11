// SDK `replay` (functional + client): reproduces a recorded session through the
// typed wrapper, stays fail-closed without approval, and maps native errors to
// AutoAgentError. The session is laid down on disk in the engine's recorded
// session format, so this exercises the real reproduce path end-to-end.
import test from "node:test";
import assert from "node:assert";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { AutoAgent, AutoAgentError, replay } from "../dist/index.js";

const PLAN = {
  objective: "build x",
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
    content: "// x\n",
  }],
  validation_commands: [],
  risks: [],
  rollback_strategy: "snapshot",
};

const SESSION_ID = "20200101T000000Z-build-x";

function recordSession(id = SESSION_ID) {
  const root = mkdtempSync(join(tmpdir(), "aa-replay-"));
  new AutoAgent(root).init();
  const sdir = join(root, ".agent", "sessions", id);
  mkdirSync(sdir, { recursive: true });
  writeFileSync(
    join(sdir, "session.json"),
    JSON.stringify({ session_id: id, objective: "build x", created: "20200101T000000Z", steps: 1 }),
  );
  writeFileSync(join(sdir, "step-001.plan.json"), JSON.stringify(PLAN));
  return { root, id };
}

test("replay reproduces the recorded change", () => {
  const { root, id } = recordSession();
  const outcome = replay(root, id, true);
  assert.strictEqual(outcome.final_state, "Completed");
  assert.ok(outcome.run_id);
  assert.strictEqual(readFileSync(join(root, "crates/x.rs"), "utf8"), "// x\n");
});

test("replay fail-closed without approval", () => {
  const { root, id } = recordSession();
  assert.throws(
    () => replay(root, id, false),
    (e) => e instanceof AutoAgentError && e.code.startsWith("policy"),
  );
  assert.ok(!existsSync(join(root, "crates/x.rs")));
});

test("replay unknown session throws AutoAgentError", () => {
  const root = mkdtempSync(join(tmpdir(), "aa-replay-"));
  new AutoAgent(root).init();
  assert.throws(() => replay(root, "nope-not-a-session", true), AutoAgentError);
});

test("client.replay reproduces", () => {
  const { root, id } = recordSession();
  const outcome = new AutoAgent(root).replay(id, true);
  assert.strictEqual(outcome.final_state, "Completed");
  assert.ok(existsSync(join(root, "crates/x.rs")));
});
