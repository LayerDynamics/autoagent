//! JSON-backed memory store (M5, SPEC-1 §9). Memory lives under `.agent/memory`
//! (not under `.agent/runs`), so it is long-lived across runs. A missing file
//! reads as empty/default — never an error.

use crate::error::{AutoAgentError, Result};
use crate::memory::schema::{
    ArchitectureNote, CommandMemory, DecisionEntry, GlossaryEntry, ProjectMemory,
};
use camino::Utf8PathBuf;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub struct MemoryStore {
    dir: Utf8PathBuf,
}

impl MemoryStore {
    pub fn new(dir: Utf8PathBuf) -> Self {
        Self { dir }
    }

    pub fn load_project(&self) -> Result<ProjectMemory> {
        self.load_or_default("project.json")
    }
    pub fn save_project(&self, p: &ProjectMemory) -> Result<()> {
        self.save("project.json", p)
    }

    pub fn load_decisions(&self) -> Result<Vec<DecisionEntry>> {
        self.load_or_default("decisions.json")
    }
    pub fn append_decision(&self, d: DecisionEntry) -> Result<()> {
        let mut all = self.load_decisions()?;
        all.push(d);
        self.save("decisions.json", &all)
    }
    pub fn remove_decision(&self, id: &str) -> Result<bool> {
        let mut all = self.load_decisions()?;
        let before = all.len();
        all.retain(|d| d.id != id);
        let removed = all.len() != before;
        self.save("decisions.json", &all)?;
        Ok(removed)
    }

    pub fn load_commands(&self) -> Result<CommandMemory> {
        self.load_or_default("commands.json")
    }
    pub fn save_commands(&self, c: &CommandMemory) -> Result<()> {
        self.save("commands.json", c)
    }

    pub fn load_architecture(&self) -> Result<Vec<ArchitectureNote>> {
        self.load_or_default("architecture.json")
    }
    pub fn save_architecture(&self, a: &[ArchitectureNote]) -> Result<()> {
        self.save("architecture.json", &a.to_vec())
    }

    pub fn load_glossary(&self) -> Result<Vec<GlossaryEntry>> {
        self.load_or_default("glossary.json")
    }
    pub fn append_glossary(&self, g: GlossaryEntry) -> Result<()> {
        let mut all = self.load_glossary()?;
        all.push(g);
        self.save("glossary.json", &all)
    }

    fn load_or_default<T: DeserializeOwned + Default>(&self, name: &str) -> Result<T> {
        let path = self.dir.join(name);
        match std::fs::read_to_string(path.as_std_path()) {
            Ok(text) => {
                serde_json::from_str(&text).map_err(|e| AutoAgentError::Memory(e.to_string()))
            }
            Err(_) => Ok(T::default()),
        }
    }

    fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<()> {
        std::fs::create_dir_all(self.dir.as_std_path())?;
        let text = serde_json::to_string_pretty(value)
            .map_err(|e| AutoAgentError::Memory(e.to_string()))?;
        std::fs::write(self.dir.join(name).as_std_path(), text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, MemoryStore) {
        let dir = tempfile::tempdir().unwrap();
        let mdir = camino::Utf8Path::from_path(dir.path())
            .unwrap()
            .join("memory");
        (dir, MemoryStore::new(mdir))
    }

    #[test]
    fn saves_and_loads_decisions() {
        let (_d, store) = store();
        store
            .append_decision(DecisionEntry {
                id: "d1".into(),
                date: "2026-06-08".into(),
                decision: "x".into(),
                rationale: "y".into(),
                run_id: None,
            })
            .unwrap();
        let loaded = store.load_decisions().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "d1");
    }

    #[test]
    fn missing_file_loads_empty() {
        let (_d, store) = store();
        assert!(store.load_decisions().unwrap().is_empty());
        assert_eq!(store.load_project().unwrap().name, "");
    }

    #[test]
    fn remove_decision_works() {
        let (_d, store) = store();
        store
            .append_decision(DecisionEntry {
                id: "d1".into(),
                date: "d".into(),
                decision: "x".into(),
                rationale: "y".into(),
                run_id: None,
            })
            .unwrap();
        assert!(store.remove_decision("d1").unwrap());
        assert!(store.load_decisions().unwrap().is_empty());
    }
}
