// The AutoAgent client class — sugar over the native functional API that holds
// the workspace root so callers pass it once.

import {
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
} from "../../crates/autoagent-bingen/deno/mod.ts";
import type {
  DoctorReport,
  EvolveOutcome,
  MemorySummary,
  ProjectAnalysis,
  RunOutcome,
} from "./_models.ts";

/** A workspace-scoped client. `new AutoAgent('/repo').doctor()`. */
export class AutoAgent {
  constructor(public readonly root: string) {}

  doctor(): DoctorReport {
    return doctor(this.root);
  }
  analyze(): ProjectAnalysis {
    return analyze(this.root);
  }
  configShow(): string {
    return configShow(this.root);
  }
  patchList(): string[] {
    return patchList(this.root);
  }
  patchShow(runId: string): string {
    return patchShow(this.root, runId);
  }
  memoryShow(): MemorySummary {
    return memoryShow(this.root);
  }
  toolsList(): string[] {
    return toolsList(this.root);
  }
  init(): boolean {
    return init(this.root);
  }
  apply(planPath: string, approve = false): string {
    return apply(this.root, planPath, approve);
  }
  revert(runId: string): void {
    revert(this.root, runId);
  }
  runSync(objective: string, from: string | null = null, approve = false): RunOutcome {
    return runSync(this.root, objective, from, approve);
  }
  evolveSync(objective: string, from: string | null = null, apply = false): EvolveOutcome {
    return evolveSync(this.root, objective, from, apply);
  }
}
