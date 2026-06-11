// SDK `replay` (functional + client): reproduces a recorded session through the
// typed wrapper, stays fail-closed without approval, and maps native errors to
// AutoAgentError. The session is laid down on disk in the engine's recorded
// session format, so this exercises the real reproduce path end-to-end. Run:
//   AUTOAGENT_BINGEN_LIB=$PWD/target/release/libautoagent_bingen.dylib \
//   deno test --allow-ffi --allow-read --allow-write --allow-env sdk/deno/replay.test.ts
import { assert, assertEquals, assertThrows } from "jsr:@std/assert@1";
import { AutoAgent, AutoAgentError, replay } from "./mod.ts";

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

function recordSession(id = SESSION_ID): { root: string; id: string } {
  const root = Deno.makeTempDirSync({ prefix: "aa-replay-" });
  new AutoAgent(root).init();
  const sdir = `${root}/.agent/sessions/${id}`;
  Deno.mkdirSync(sdir, { recursive: true });
  Deno.writeTextFileSync(
    `${sdir}/session.json`,
    JSON.stringify({ session_id: id, objective: "build x", created: "20200101T000000Z", steps: 1 }),
  );
  Deno.writeTextFileSync(`${sdir}/step-001.plan.json`, JSON.stringify(PLAN));
  return { root, id };
}

Deno.test("replay reproduces the recorded change", () => {
  const { root, id } = recordSession();
  const outcome = replay(root, id, true);
  assertEquals(outcome.final_state, "Completed");
  assert(outcome.run_id);
  assertEquals(Deno.readTextFileSync(`${root}/crates/x.rs`), "// x\n");
});

Deno.test("replay fail-closed without approval", () => {
  const { root, id } = recordSession();
  assertThrows(() => replay(root, id, false), AutoAgentError);
  assert(!existsSync(`${root}/crates/x.rs`));
});

Deno.test("replay unknown session throws", () => {
  const root = Deno.makeTempDirSync({ prefix: "aa-replay-" });
  new AutoAgent(root).init();
  assertThrows(() => replay(root, "nope-not-a-session", true), AutoAgentError);
});

Deno.test("client.replay reproduces", () => {
  const { root, id } = recordSession();
  const outcome = new AutoAgent(root).replay(id, true);
  assertEquals(outcome.final_state, "Completed");
  assert(Deno.statSync(`${root}/crates/x.rs`).isFile);
});

function existsSync(path: string): boolean {
  try {
    Deno.statSync(path);
    return true;
  } catch {
    return false;
  }
}
