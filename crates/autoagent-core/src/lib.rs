//! autoagent-core — the policy-controlled mutation engine for AutoAgent.
//!
//! All privileged logic (config, analysis, planning, editing, validation,
//! safety, memory, logging, git, errors) lives here so it can be tested
//! independently of the CLI. Modules are added milestone by milestone.

pub mod analysis;
pub mod config;
pub mod editing;
pub mod error;
pub mod logging;
pub mod planning;
pub mod runtime;
pub mod safety;
pub mod validation;
