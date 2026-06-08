//! AutoAgent CLI — the thin, user-facing front end. All privileged logic lives
//! in `autoagent-core`; this crate parses arguments, renders output, asks for
//! confirmations, and maps errors to stable process exit codes (SPEC-1 §3.11).

mod approval;
mod commands;

use approval::DialoguerGate;
use autoagent_core::error::{AutoAgentError, Result};
use autoagent_core::runtime::{agent_loop, doctor, init, revert};
use autoagent_core::safety::approval_gate::{ApprovalGate, AutoGate};
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "autoagent",
    version,
    about = "Safe, reversible, policy-controlled codebase evolution"
)]
struct Cli {
    /// Approve writes/commands without prompting.
    #[arg(long, global = true)]
    yes: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize AutoAgent in the current workspace.
    Init,
    /// Check local system, config, commands, and workspace health.
    Doctor,
    /// Apply a structured plan through the policy-controlled mutation engine.
    Apply { plan: PathBuf },
    /// Revert a previous AutoAgent run.
    Revert { run_id: String },
    /// List or show patch artifacts.
    Patch {
        #[command(subcommand)]
        sub: PatchCmd,
    },
    /// Show or validate Autoagent.toml.
    Config {
        #[command(subcommand)]
        sub: ConfigCmd,
    },
}

#[derive(Subcommand)]
enum PatchCmd {
    List,
    Show { run_id: String },
}

#[derive(Subcommand)]
enum ConfigCmd {
    Show,
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error [{}]: {e}", e.error_code());
        std::process::exit(e.exit_code());
    }
}

fn run(cli: Cli) -> Result<()> {
    let root = cwd()?;
    let auto = AutoGate::allow();
    let interactive = DialoguerGate;
    let gate: &dyn ApprovalGate = if cli.yes { &auto } else { &interactive };

    match cli.command {
        Command::Init => {
            let wrote = init::init_workspace(&root)?;
            if wrote {
                println!("initialized AutoAgent: wrote Autoagent.toml and .agent/");
            } else {
                println!(
                    "AutoAgent already initialized (Autoagent.toml preserved); ensured .agent/"
                );
            }
        }
        Command::Doctor => {
            let report = doctor::doctor(&root);
            for c in &report.checks {
                let mark = if c.ok { "ok" } else { "FAIL" };
                println!("[{mark}] {}: {}", c.name, c.detail);
            }
            if !report.all_ok() {
                return Err(AutoAgentError::Workspace(
                    "one or more health checks failed".into(),
                ));
            }
        }
        Command::Apply { plan } => {
            let plan_path = to_utf8(plan)?;
            let run_id = agent_loop::apply_with_gate(&root, &plan_path, gate)?;
            println!("applied run {run_id}");
        }
        Command::Revert { run_id } => {
            revert::revert(&root, &run_id)?;
            println!("reverted run {run_id}");
        }
        Command::Patch { sub } => match sub {
            PatchCmd::List => commands::patch_list(&root)?,
            PatchCmd::Show { run_id } => commands::patch_show(&root, &run_id)?,
        },
        Command::Config { sub } => match sub {
            ConfigCmd::Show => commands::config_show(&root)?,
        },
    }
    Ok(())
}

fn cwd() -> Result<Utf8PathBuf> {
    let p = std::env::current_dir().map_err(AutoAgentError::Io)?;
    Utf8PathBuf::from_path_buf(p)
        .map_err(|_| AutoAgentError::Workspace("non-utf8 working directory".into()))
}

fn to_utf8(p: PathBuf) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(p).map_err(|_| AutoAgentError::Plan("non-utf8 plan path".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_apply_with_path() {
        let cli = Cli::try_parse_from(["autoagent", "apply", "p.plan.json"]).unwrap();
        assert!(matches!(cli.command, Command::Apply { .. }));
    }

    #[test]
    fn parses_yes_flag() {
        let cli = Cli::try_parse_from(["autoagent", "--yes", "init"]).unwrap();
        assert!(cli.yes);
    }

    #[test]
    fn parses_patch_show() {
        let cli = Cli::try_parse_from(["autoagent", "patch", "show", "run-1"]).unwrap();
        match cli.command {
            Command::Patch {
                sub: PatchCmd::Show { run_id },
            } => assert_eq!(run_id, "run-1"),
            _ => panic!("wrong command"),
        }
    }
}
