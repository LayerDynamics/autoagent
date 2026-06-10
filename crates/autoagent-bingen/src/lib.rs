//! autoagent-bingen — generates Python, Node.js, and Deno bindings for
//! `autoagent-core` from a single surface registry (`bind.rs`).
//!
//! `bind.rs` is the single source of truth: it declares the exported surface and
//! holds the backend-neutral marshaling, error mapping, approval bridge, and
//! async helpers. The `bingen` binary (`main.rs`) reads it and generates the six
//! backend adapters + type stubs + JSON schema + package scaffolds.
//!
//! See `docs/specs/SPEC-2-autoagent-bingen.md`.

#[path = "../bind.rs"]
pub mod bind;

pub mod gen;

pub mod deno;
pub mod node;
pub mod python;
