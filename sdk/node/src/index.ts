// AutoAgent — typed Node.js SDK over the native bindings (@autoagent/native).
// The native binding returns parsed objects (napi) and throws `[code|exitCode]`
// errors; this layer types the results with the generated models and maps
// errors to AutoAgentError. Mutating ops preserve fail-closed safety.

import nativeDefault from "@autoagent/native";
import { AutoAgentError } from "./errors.js";
import type {
  DoctorReport,
  EvolveOutcome,
  MemorySummary,
  ProjectAnalysis,
  RunOutcome,
} from "./_models.js";

export * from "./_models.js";
export { AutoAgentError };
export { AutoAgent } from "./client.js";

// @autoagent/native is the runtime bridge; the SDK supplies the real types via
// the generated models, so the native is consumed loosely and cast.
// deno-lint-ignore no-explicit-any
const native = nativeDefault as any;

function wrap<T>(fn: () => T): T {
  try {
    return fn();
  } catch (e) {
    throw AutoAgentError.fromNative(e);
  }
}

// --- read surface ---------------------------------------------------------

/** Schema version this build supports. */
export function version(): number {
  return native.version();
}

/** System, config, and workspace health checks. */
export function doctor(root: string): DoctorReport {
  return wrap(() => native.doctor(root)) as DoctorReport;
}

/** Analyze the project and write its report. */
export function analyze(root: string): ProjectAnalysis {
  return wrap(() => native.analyze(root)) as ProjectAnalysis;
}

/** Render the effective Autoagent.toml (raw TOML). */
export function configShow(root: string): string {
  return wrap(() => native.configShow(root));
}

/** List patch-artifact run ids. */
export function patchList(root: string): string[] {
  return wrap(() => native.patchList(root));
}

/** Show a patch body (raw unified diff). */
export function patchShow(root: string, runId: string): string {
  return wrap(() => native.patchShow(root, runId));
}

/** Project-memory summary. */
export function memoryShow(root: string): MemorySummary {
  return wrap(() => native.memoryShow(root)) as MemorySummary;
}

/** Registered plugin tools. */
export function toolsList(root: string): string[] {
  return wrap(() => native.toolsList(root));
}

// --- mutating surface (fail-closed) ---------------------------------------

/** Initialize Autoagent.toml + the .agent/ tree. */
export function init(root: string): boolean {
  return wrap(() => native.init(root));
}

/** Apply a plan through the policy engine; returns the run id. Throws
 * AutoAgentError when `approve` is false and the config requires it. */
export function apply(root: string, planPath: string, approve = false): string {
  return wrap(() => native.apply(root, planPath, approve));
}

/** Revert a previous run. */
export function revert(root: string, runId: string): void {
  wrap(() => native.revert(root, runId));
}

/** Supervised run (blocking). */
export function runSync(
  root: string,
  objective: string,
  from: string | null = null,
  approve = false,
): RunOutcome {
  return wrap(() => native.runSync(root, objective, from, approve)) as RunOutcome;
}

/** Controlled self-authoring (blocking); `apply` is gated by allow_self_modification. */
export function evolveSync(
  root: string,
  objective: string,
  from: string | null = null,
  apply = false,
): EvolveOutcome {
  return wrap(() => native.evolveSync(root, objective, from, apply)) as EvolveOutcome;
}

/** Supervised run (Promise). */
export async function run(
  root: string,
  objective: string,
  from: string | null = null,
  approve = false,
): Promise<RunOutcome> {
  try {
    return (await native.run(root, objective, from, approve)) as RunOutcome;
  } catch (e) {
    throw AutoAgentError.fromNative(e);
  }
}

/** Controlled self-authoring (Promise). */
export async function evolve(
  root: string,
  objective: string,
  from: string | null = null,
  apply = false,
): Promise<EvolveOutcome> {
  try {
    return (await native.evolve(root, objective, from, apply)) as EvolveOutcome;
  } catch (e) {
    throw AutoAgentError.fromNative(e);
  }
}
