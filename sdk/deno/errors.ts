// The SDK re-exports the native binding's AutoAgentError so `instanceof` checks
// hold across the boundary. It carries the stable `code` and numeric `exitCode`
// from core's taxonomy (FR-8). For a published build this import resolves to
// `jsr:@autoagent/native`; locally it is the workspace binding.
export { AutoAgentError } from "../../crates/autoagent-bingen/deno/mod.ts";
