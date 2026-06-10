// Deno SDK tests (S4). Run:
//   AUTOAGENT_BINGEN_LIB=$PWD/target/release/libautoagent_bingen.dylib \
//   deno test --allow-ffi --allow-read --allow-write --allow-env sdk/deno/mod.test.ts
import { assert, assertEquals, assertThrows } from "jsr:@std/assert@1";
import { AutoAgent, AutoAgentError, doctor, version } from "./mod.ts";

function seed(): { root: string; plan: string } {
  const root = Deno.makeTempDirSync({ prefix: "aa-deno-sdk-" });
  new AutoAgent(root).init();
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
  return { root, plan };
}

Deno.test("version is 1", () => {
  assertEquals(version(), 1);
});

Deno.test("doctor returns a typed report", () => {
  const r = doctor(Deno.makeTempDirSync());
  assert(Array.isArray(r.checks));
});

Deno.test("client apply fail-closed without approval", () => {
  const { root, plan } = seed();
  const aa = new AutoAgent(root);
  assertThrows(
    () => aa.apply(plan, false),
    AutoAgentError,
  );
});

Deno.test("client apply + revert roundtrip", () => {
  const { root, plan } = seed();
  const aa = new AutoAgent(root);
  const runId = aa.apply(plan, true);
  assert(runId);
  assert(Deno.statSync(`${root}/crates/x.rs`).isFile);
  aa.revert(runId);
});
