//! Repair context + step budget (M4). The repair pass re-plans against a
//! failure, bounded by `[agent].max_steps_per_run` (SPEC-1 FR-25).
//!
//! PROPOSED DESIGN: the failure-context format is an M4 decision.

use crate::validation::validation_report::ValidationReport;

const ERROR_EXCERPT_LINES: usize = 40;

#[derive(Debug, Clone)]
pub struct RepairContext {
    pub failing_command: String,
    pub error_excerpt: String,
}

impl RepairContext {
    pub fn from_failure(rep: &ValidationReport) -> Self {
        match rep.commands.iter().find(|c| c.exit_code != Some(0)) {
            Some(c) => Self {
                failing_command: c.command.clone(),
                error_excerpt: tail_lines(
                    &format!("{}\n{}", c.stdout, c.stderr),
                    ERROR_EXCERPT_LINES,
                ),
            },
            None => Self {
                failing_command: String::new(),
                error_excerpt: String::new(),
            },
        }
    }
}

/// Bounds the number of repair attempts to `max_steps_per_run`.
pub struct StepBudget {
    remaining: u32,
}

impl StepBudget {
    pub fn new(max: u32) -> Self {
        Self { remaining: max }
    }
    pub fn try_consume(&mut self) -> bool {
        if self.remaining == 0 {
            false
        } else {
            self.remaining -= 1;
            true
        }
    }
    pub fn remaining(&self) -> u32 {
        self.remaining
    }
}

fn tail_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::validation_report::CommandValidationResult;

    fn failed_report() -> ValidationReport {
        ValidationReport {
            passed: false,
            commands: vec![CommandValidationResult {
                command: "cargo test".into(),
                exit_code: Some(101),
                stdout: "running".into(),
                stderr: "error[E0599]: no method `foo`".into(),
                duration_ms: 5,
            }],
        }
    }

    #[test]
    fn repair_context_summarizes_failures() {
        let ctx = RepairContext::from_failure(&failed_report());
        assert!(ctx.failing_command.contains("cargo test"));
        assert!(ctx.error_excerpt.contains("E0599"));
    }

    #[test]
    fn budget_decrements_and_stops() {
        let mut b = StepBudget::new(2);
        assert!(b.try_consume());
        assert!(b.try_consume());
        assert!(!b.try_consume());
        assert_eq!(b.remaining(), 0);
    }
}
