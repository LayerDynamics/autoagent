//! `bingen` — the binding generator CLI. Reads the surface registry (`bind.rs`)
//! and generates the backend adapters, type stubs, JSON schema, and package
//! scaffolds; `check` is the drift guard; `smoke` loads a built backend.

use autoagent_bingen::gen;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    let result = match cmd.as_str() {
        "generate" => gen::generate(),
        "check" => gen::check(),
        "smoke" => gen::smoke(),
        other => {
            eprintln!("usage: bingen [generate|check|smoke] (got {other:?})");
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
