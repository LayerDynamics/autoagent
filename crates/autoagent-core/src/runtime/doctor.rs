//! `doctor` — read-only health checks of toolchain, config, and workspace
//! (SPEC-1 FR-3). Performs no writes; only inspection and version probes.

use crate::config::config_schema::AutoAgentConfig;
use camino::Utf8Path;

#[derive(Debug, Clone)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub checks: Vec<Check>,
}

impl DoctorReport {
    pub fn all_ok(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }
}

pub fn doctor(root: &Utf8Path) -> DoctorReport {
    let mut checks = Vec::new();

    checks.push(tool_check("rust", "rustc", "--version"));
    checks.push(tool_check("cargo", "cargo", "--version"));
    checks.push(tool_check("git", "git", "--version"));

    // Config presence + validity (read-only).
    let config = AutoAgentConfig::load(root);
    checks.push(match &config {
        Ok(_) => Check {
            name: "config".into(),
            ok: true,
            detail: "Autoagent.toml present and valid".into(),
        },
        Err(e) => Check {
            name: "config".into(),
            ok: false,
            detail: e.to_string(),
        },
    });

    // Workspace writability (read metadata only — no writes).
    let writable = std::fs::metadata(root.as_std_path())
        .map(|m| !m.permissions().readonly())
        .unwrap_or(false);
    checks.push(Check {
        name: "workspace".into(),
        ok: writable,
        detail: if writable {
            "workspace root is writable".into()
        } else {
            "workspace root is not writable".into()
        },
    });

    // .agent presence.
    let agent_ok = root.join(".agent").as_std_path().is_dir();
    checks.push(Check {
        name: "agent_dir".into(),
        ok: agent_ok,
        detail: if agent_ok {
            ".agent workspace present".into()
        } else {
            ".agent workspace missing (run `autoagent init`)".into()
        },
    });

    // Availability of each configured [commands] entry's binary (FR-3).
    if let Ok(cfg) = &config {
        for (label, cmd) in [
            ("cmd:test", &cfg.commands.test),
            ("cmd:lint", &cfg.commands.lint),
            ("cmd:format", &cfg.commands.format),
            ("cmd:build", &cfg.commands.build),
        ] {
            checks.push(command_available(label, cmd));
        }
    }

    DoctorReport { checks }
}

/// Check that a configured command's executable is resolvable on PATH, without
/// running the command itself (read-only).
fn command_available(label: &str, command: &str) -> Check {
    let bin = command.split_whitespace().next().unwrap_or("");
    let found = which_in_path(bin);
    Check {
        name: label.into(),
        ok: found,
        detail: if found {
            format!("`{bin}` available for `{command}`")
        } else {
            format!("`{bin}` not found on PATH for `{command}`")
        },
    }
}

/// Resolve `bin` against PATH entries (read-only; no execution).
fn which_in_path(bin: &str) -> bool {
    if bin.is_empty() {
        return false;
    }
    match std::env::var_os("PATH") {
        Some(paths) => std::env::split_paths(&paths).any(|dir| {
            let candidate = dir.join(bin);
            candidate.is_file()
                || candidate.with_extension("exe").is_file()
                || candidate.with_extension("cmd").is_file()
        }),
        None => false,
    }
}

fn tool_check(name: &str, bin: &str, arg: &str) -> Check {
    match std::process::Command::new(bin).arg(arg).output() {
        Ok(out) if out.status.success() => Check {
            name: name.into(),
            ok: true,
            detail: String::from_utf8_lossy(&out.stdout).trim().to_string(),
        },
        Ok(_) => Check {
            name: name.into(),
            ok: false,
            detail: format!("{bin} present but {arg} failed"),
        },
        Err(_) => Check {
            name: name.into(),
            ok: false,
            detail: format!("{bin} not found on PATH"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_reports_config_presence() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        crate::runtime::init::init_workspace(root).unwrap();
        let report = doctor(root);
        assert!(report.checks.iter().any(|c| c.name == "config" && c.ok));
        assert!(report.checks.iter().any(|c| c.name == "agent_dir" && c.ok));
    }

    #[test]
    fn doctor_flags_missing_config() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let report = doctor(root);
        assert!(report.checks.iter().any(|c| c.name == "config" && !c.ok));
    }
}
