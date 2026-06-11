//! Redactor (M3, SPEC-1 FR-22 / OQ-4) — the enforcement point for sensitive
//! egress. Excluded/secret files are never forwarded to a provider, and secret
//! lines in forwarded content are scrubbed.
//!
//! PROPOSED DESIGN: the exact secret patterns are an M3 first cut (OQ-4).

use globset::{Glob, GlobSet, GlobSetBuilder};

const BUILTIN_SECRET_GLOBS: &[&str] = &[".env*", "*.pem", "*.key", "id_rsa*", "*.p12"];
const SECRET_KEYWORDS: &[&str] = &["api_key", "apikey", "secret", "token", "password"];

pub struct Redactor {
    excluded: GlobSet,
}

impl Redactor {
    /// Build from the workspace `exclude` globs plus the built-in secret globs.
    pub fn new(exclude: Vec<String>) -> Self {
        let mut b = GlobSetBuilder::new();
        for g in exclude
            .iter()
            .map(String::as_str)
            .chain(BUILTIN_SECRET_GLOBS.iter().copied())
        {
            if let Ok(glob) = Glob::new(g) {
                b.add(glob);
            }
        }
        Self {
            excluded: b.build().unwrap_or_else(|_| GlobSet::empty()),
        }
    }

    /// True if a path must never be forwarded to a provider. Matches the glob
    /// set against both the full relative path and the bare file name.
    pub fn is_excluded(&self, path: &str) -> bool {
        if self.excluded.is_match(path) {
            return true;
        }
        let base = path.rsplit('/').next().unwrap_or(path);
        self.excluded.is_match(base)
    }

    /// Redact secret-looking values from text before it leaves the machine.
    pub fn scrub(&self, text: &str) -> String {
        let mut out: Vec<String> = Vec::new();
        for line in text.lines() {
            let lower = line.to_lowercase();
            let looks_secret = SECRET_KEYWORDS.iter().any(|k| lower.contains(k));
            if looks_secret {
                if let Some(pos) = line.find(['=', ':']) {
                    out.push(format!("{}{}<redacted>", &line[..pos], &line[pos..=pos]));
                    continue;
                }
            }
            out.push(line.to_string());
        }
        out.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_excluded_paths_and_secret_lines() {
        let r = Redactor::new(vec![".env".into(), "*.pem".into()]);
        assert!(r.is_excluded("config/.env"));
        assert!(r.is_excluded("certs/server.pem"));
        let cleaned = r.scrub("API_KEY=sk-secret\nfn main(){}");
        assert!(!cleaned.contains("sk-secret"));
        assert!(cleaned.contains("fn main"));
    }

    #[test]
    fn non_secret_code_is_untouched() {
        let r = Redactor::new(vec![]);
        assert!(!r.is_excluded("crates/lib.rs"));
        let code = "let x = 1;\nfn add(a: i32) -> i32 { a + 1 }";
        assert_eq!(r.scrub(code), code);
    }

    #[test]
    fn builtin_env_glob_excludes_dotenv_variants() {
        let r = Redactor::new(vec![]);
        assert!(r.is_excluded(".env.local"));
        assert!(r.is_excluded("id_rsa"));
    }

    #[test]
    fn scrubs_colon_delimiter_and_every_secret_keyword() {
        let r = Redactor::new(vec![]);
        // JSON-style `:` delimiter is redacted, not just `=`.
        let json = r.scrub("\"token\": \"abc123\"");
        assert!(
            !json.contains("abc123"),
            "colon-delimited secret leaked: {json}"
        );
        // Each recognized keyword triggers redaction.
        for kw in ["api_key", "apikey", "secret", "token", "password"] {
            let line = format!("{kw} = topsecretvalue");
            assert!(
                !r.scrub(&line).contains("topsecretvalue"),
                "keyword `{kw}` did not redact"
            );
        }
    }

    #[test]
    fn keyword_match_is_case_insensitive() {
        let r = Redactor::new(vec![]);
        assert!(!r.scrub("PASSWORD=hunter2").contains("hunter2"));
        assert!(!r.scrub("ApiKey: zzz").contains("zzz"));
    }

    #[test]
    fn secret_keyword_without_a_delimiter_is_left_intact() {
        // Pins the M3 first-cut behavior (OQ-4): redaction keys off `=`/`:`, so a
        // secret mentioned in prose without a delimiter is NOT scrubbed. This
        // documents the current limit and guards against silent behavior change.
        let r = Redactor::new(vec![]);
        let line = "the password is hunter2";
        assert_eq!(r.scrub(line), line);
    }
}
