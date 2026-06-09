// Smoke test (FR-12): load the freshly built napi cdylib straight from the
// workspace target dir and exercise a non-mutating call (doctor).
//
// B1 loads the raw cdylib via `process.dlopen` (napi modules self-register on
// load). B5-T2 replaces this with the real `*.node` artifact + loader shim.

import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
// crates/autoagent-bingen/__test__ -> repo root -> target/release
const targetDir = join(here, "..", "..", "..", "target", "release");

const ext =
  process.platform === "darwin"
    ? "dylib"
    : process.platform === "win32"
    ? "dll"
    : "so";
const prefix = process.platform === "win32" ? "" : "lib";
const libPath = join(targetDir, `${prefix}autoagent_bingen.${ext}`);

if (!existsSync(libPath)) {
  console.error(`smoke: cdylib not found at ${libPath}`);
  process.exit(1);
}

const mod = { exports: {} };
process.dlopen(mod, libPath);
const addon = mod.exports;

// version() -> number
const version = addon.version();
if (version !== 1) {
  console.error(`smoke: unexpected version ${version}`);
  process.exit(1);
}

// doctor(root) -> object with checks[]
const report = addon.doctor(tmpdir());
if (!report || !Array.isArray(report.checks)) {
  console.error("smoke: doctor() returned a bad shape:", report);
  process.exit(1);
}

console.log(`smoke ok: version=${version}, doctor checks=${report.checks.length}`);
