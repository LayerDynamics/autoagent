//! Unified diff builder (SPEC-1 §3.2 editing) backed by `similar`.

use similar::TextDiff;

/// Produce a unified diff between `before` and `after` for `path`.
pub fn unified(before: &str, after: &str, path: &str) -> String {
    let diff = TextDiff::from_lines(before, after);
    diff.unified_diff()
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_shows_changed_line() {
        let d = unified("hello\nworld\n", "hello\nrust\n", "x.txt");
        assert!(d.contains("-world"));
        assert!(d.contains("+rust"));
        assert!(d.contains("a/x.txt"));
    }
}
