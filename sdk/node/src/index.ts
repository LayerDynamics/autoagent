// AutoAgent — typed Node.js SDK over the native bindings (@autoagent/native).
// Fleshed out in S3-T2 (functional API + errors) and S3-T3 (client class).
export * from "./_models.js";

// @autoagent/native is CJS (`module.exports = require(...)`), so use a default
// import — its named exports aren't statically detectable by Node's ESM loader.
import native from "@autoagent/native";

/** Schema version this build supports. */
export function version(): number {
  return native.version();
}
