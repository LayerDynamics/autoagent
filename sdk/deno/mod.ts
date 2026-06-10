// AutoAgent — typed Deno SDK over the native FFI binding.
//
// The native binding (`@autoagent/native`) already returns typed models and
// throws `AutoAgentError`; this module re-exports that functional API with the
// SDK's models + error type and adds the `AutoAgent` client class. Requires
// --allow-ffi (and --allow-read/--allow-write for mutating ops); set
// AUTOAGENT_BINGEN_LIB to the built cdylib.
//
// NOTE: async run/evolve are exposed via the deno_bindgen path (non_blocking);
// this FFI SDK surfaces the synchronous `runSync`/`evolveSync`.

export * from "./_models.ts";
export { AutoAgentError } from "./errors.ts";
export { AutoAgent } from "./client.ts";

export {
  analyze,
  apply,
  configShow,
  doctor,
  evolveSync,
  init,
  memoryShow,
  patchList,
  patchShow,
  revert,
  runSync,
  toolsList,
  version,
} from "../../crates/autoagent-bingen/deno/mod.ts";
