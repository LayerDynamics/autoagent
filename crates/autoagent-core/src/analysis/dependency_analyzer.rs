//! Dependency parsing (M2) for Cargo and npm manifests. Direct dependencies
//! only (PROPOSED: transitive/lockfile resolution is out of M2 scope).

use crate::analysis::project_analysis::DependencySummary;
use crate::error::{AutoAgentError, Result};

/// Parse `[dependencies]` and `[dev-dependencies]` from a `Cargo.toml` string.
pub fn parse_cargo(toml_src: &str) -> Result<Vec<DependencySummary>> {
    let value: toml::Value =
        toml::from_str(toml_src).map_err(|e| AutoAgentError::Analysis(e.to_string()))?;
    let mut out = Vec::new();
    collect_cargo(&value, "dependencies", false, &mut out);
    collect_cargo(&value, "dev-dependencies", true, &mut out);
    Ok(out)
}

fn collect_cargo(root: &toml::Value, table: &str, dev: bool, out: &mut Vec<DependencySummary>) {
    if let Some(deps) = root.get(table).and_then(|v| v.as_table()) {
        for (name, spec) in deps {
            let version = match spec {
                toml::Value::String(s) => s.clone(),
                toml::Value::Table(t) => t
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*")
                    .to_string(),
                _ => "*".to_string(),
            };
            out.push(DependencySummary {
                name: name.clone(),
                version,
                dev,
            });
        }
    }
}

/// Parse `dependencies` and `devDependencies` from a `package.json` string.
pub fn parse_package_json(json_src: &str) -> Result<Vec<DependencySummary>> {
    let value: serde_json::Value =
        serde_json::from_str(json_src).map_err(|e| AutoAgentError::Analysis(e.to_string()))?;
    let mut out = Vec::new();
    collect_npm(&value, "dependencies", false, &mut out);
    collect_npm(&value, "devDependencies", true, &mut out);
    Ok(out)
}

fn collect_npm(root: &serde_json::Value, key: &str, dev: bool, out: &mut Vec<DependencySummary>) {
    if let Some(deps) = root.get(key).and_then(|v| v.as_object()) {
        for (name, spec) in deps {
            out.push(DependencySummary {
                name: name.clone(),
                version: spec.as_str().unwrap_or("*").to_string(),
                dev,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cargo_deps() {
        let toml = r#"[package]
name="x"
version="0.1.0"
[dependencies]
serde="1"
clap={version="4",features=["derive"]}
[dev-dependencies]
proptest="1""#;
        let deps = parse_cargo(toml).unwrap();
        assert!(deps
            .iter()
            .any(|d| d.name == "serde" && !d.dev && d.version == "1"));
        assert!(deps.iter().any(|d| d.name == "clap" && d.version == "4"));
        assert!(deps.iter().any(|d| d.name == "proptest" && d.dev));
    }

    #[test]
    fn parses_npm_deps() {
        let json = r#"{"name":"x","dependencies":{"react":"18"},"devDependencies":{"vitest":"1"}}"#;
        let deps = parse_package_json(json).unwrap();
        assert!(deps.iter().any(|d| d.name == "react" && !d.dev));
        assert!(deps.iter().any(|d| d.name == "vitest" && d.dev));
    }
}
