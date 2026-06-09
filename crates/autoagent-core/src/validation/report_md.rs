//! Validation report Markdown writer (M4, SPEC-1 FR-11 / §9.1).

use crate::validation::validation_report::ValidationReport;
use std::fmt::Write as _;

const STDERR_TAIL_LINES: usize = 20;

pub fn render_report(rep: &ValidationReport) -> String {
    let mut s = String::new();
    let status = if rep.passed { "PASSED" } else { "FAILED" };
    let _ = writeln!(s, "# Validation Report — {status}\n");

    let _ = writeln!(s, "| Command | Exit | Duration (ms) | Status |");
    let _ = writeln!(s, "| --- | --- | --- | --- |");
    for c in &rep.commands {
        let ok = c.exit_code == Some(0);
        let _ = writeln!(
            s,
            "| `{}` | {} | {} | {} |",
            c.command,
            c.exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "n/a".into()),
            c.duration_ms,
            if ok { "ok" } else { "FAIL" }
        );
    }
    let _ = writeln!(s);

    for c in rep.commands.iter().filter(|c| c.exit_code != Some(0)) {
        let _ = writeln!(s, "## Failure: `{}`\n", c.command);
        let _ = writeln!(s, "```text");
        let _ = writeln!(s, "{}", tail(&c.stderr, STDERR_TAIL_LINES));
        let _ = writeln!(s, "```\n");
    }
    s
}

fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::validation_report::CommandValidationResult;

    #[test]
    fn renders_pass_fail_table() {
        let rep = ValidationReport {
            passed: false,
            commands: vec![
                CommandValidationResult {
                    command: "cargo build".into(),
                    exit_code: Some(0),
                    stdout: "".into(),
                    stderr: "".into(),
                    duration_ms: 10,
                },
                CommandValidationResult {
                    command: "cargo test".into(),
                    exit_code: Some(101),
                    stdout: "".into(),
                    stderr: "boom".into(),
                    duration_ms: 20,
                },
            ],
        };
        let md = render_report(&rep);
        assert!(md.contains("FAILED"));
        assert!(md.contains("FAIL"));
        assert!(md.contains("cargo test"));
        assert!(md.contains("boom"));
    }
}
