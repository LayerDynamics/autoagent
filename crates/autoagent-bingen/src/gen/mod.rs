//! `bingen` generation entrypoints.
//!
//! - `generate` writes every file produced by [`emit::render_all`] to disk.
//! - `check` is the drift guard: it fails if any on-disk file differs from a
//!   fresh regeneration (FR-15 / R-2).
//! - `smoke` builds the napi backend and loads it from Node to prove the
//!   binding wiring works (FR-12).

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub mod emit;

pub use emit::render_all;

/// Crate root (`crates/autoagent-bingen`) — the base for all generated paths.
const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// Format Rust source through `rustfmt` so generated files pass `cargo fmt
/// --check`. Returns the input unchanged if rustfmt is unavailable or errors.
fn rustfmt(content: &str) -> String {
    let mut child = match Command::new("rustfmt")
        .args(["--edition", "2021", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return content.to_string(),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(content.as_bytes());
    }
    match child.wait_with_output() {
        Ok(out) if out.status.success() => {
            String::from_utf8(out.stdout).unwrap_or_else(|_| content.to_string())
        }
        _ => content.to_string(),
    }
}

/// The canonical generated files: every `.rs` output passed through rustfmt so
/// the committed golden, `generate`, and `check` all agree byte-for-byte.
fn finalized() -> BTreeMap<String, String> {
    render_all()
        .into_iter()
        .map(|(rel, content)| {
            let content = if rel.ends_with(".rs") {
                rustfmt(&content)
            } else {
                content
            };
            (rel, content)
        })
        .collect()
}

/// Write all generated files to disk, creating parent directories as needed.
pub fn generate() -> Result<()> {
    let files = finalized();
    for (rel, content) in &files {
        let path = Path::new(ROOT).join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("create dir for {rel}"))?;
        }
        std::fs::write(&path, content).with_context(|| format!("write {rel}"))?;
        println!("generated {rel}");
    }
    println!("generated {} file(s)", files.len());
    Ok(())
}

/// Drift guard: fail if any generated file on disk differs from a fresh render.
pub fn check() -> Result<()> {
    let mut drift = Vec::new();
    for (rel, content) in finalized() {
        let path = Path::new(ROOT).join(&rel);
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        if on_disk != content {
            drift.push(rel);
        }
    }
    if drift.is_empty() {
        println!("no drift");
        Ok(())
    } else {
        anyhow::bail!("generated files out of date (run `bingen generate`): {drift:?}")
    }
}

/// Build the napi backend and load it from Node, exercising a non-mutating call.
pub fn smoke() -> Result<()> {
    // Build ONLY the cdylib: napi runtime symbols (`napi_*`) are resolved by
    // Node at load time via the cdylib's `dynamic_lookup` link args, which do
    // not apply to the `bingen` bin — so the bin must never link a backend.
    let status = std::process::Command::new("cargo")
        .args([
            "build",
            "-p",
            "autoagent-bingen",
            "--features",
            "node-napi",
            "--release",
            "--lib",
        ])
        .status()
        .context("spawn cargo build for napi smoke")?;
    anyhow::ensure!(status.success(), "napi build failed");

    let script = Path::new(ROOT).join("__test__/smoke.mjs");
    let node = std::process::Command::new("node")
        .arg(&script)
        .status()
        .context("spawn node for smoke")?;
    anyhow::ensure!(node.success(), "node smoke failed");
    Ok(())
}
