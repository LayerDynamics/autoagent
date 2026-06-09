//! Memory schemas (M5). PROPOSED DESIGN: SPEC-1 §9 names the memory files but
//! not their schemas; these are an M5 decision.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectMemory {
    pub name: String,
    pub language: String,
    pub package_manager: Option<String>,
    pub last_analyzed: Option<String>,
    pub source_file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEntry {
    pub id: String,
    pub date: String,
    pub decision: String,
    pub rationale: String,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandMemory {
    pub known_good: Vec<String>,
    pub known_failing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureNote {
    pub area: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlossaryEntry {
    pub term: String,
    pub definition: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_entry_roundtrips() {
        let d = DecisionEntry {
            id: "d-1".into(),
            date: "2026-06-08".into(),
            decision: "use TOML".into(),
            rationale: "safe".into(),
            run_id: Some("r1".into()),
        };
        let j = serde_json::to_string(&d).unwrap();
        let back: DecisionEntry = serde_json::from_str(&j).unwrap();
        assert_eq!(back.decision, "use TOML");
        assert_eq!(back.run_id.as_deref(), Some("r1"));
    }
}
