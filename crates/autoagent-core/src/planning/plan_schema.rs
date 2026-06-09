//! Exported JSON Schema for the `Plan` contract (M8, SPEC-1 §3.4.1).
//!
//! The schema is the FROZEN machine contract at 1.0.0. `plan_json_schema()` is
//! the single source of truth; the `schemas/plan.schema.json` golden file is
//! snapshot-locked against it, so any post-1.0 change to the schema fails CI and
//! forces a deliberate `schema_version` bump.

/// The canonical JSON Schema (draft-07) for an AutoAgent plan.
pub fn plan_json_schema() -> &'static str {
    include_str!("../../../../schemas/plan.schema.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_schema_is_valid_json_and_describes_plan() {
        let v: serde_json::Value = serde_json::from_str(plan_json_schema()).unwrap();
        assert_eq!(v["title"], "AutoAgent Plan");
        // rollback_strategy is frozen to the single supported value.
        assert_eq!(v["properties"]["rollback_strategy"]["enum"][0], "snapshot");
        // all seven operation kinds are present in the frozen enum.
        let kinds = &v["definitions"]["FileOperation"]["properties"]["kind"]["enum"];
        assert_eq!(kinds.as_array().unwrap().len(), 7);
    }

    #[test]
    fn schema_matches_frozen_golden() {
        // The function returns the golden file directly; this guards that the
        // golden remains parseable and that any drift is a deliberate edit.
        let generated: serde_json::Value = serde_json::from_str(plan_json_schema()).unwrap();
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../../../../schemas/plan.schema.json")).unwrap();
        assert_eq!(
            generated, golden,
            "Plan schema drift — a BREAKING change after 1.0.0; bump schema_version and the golden deliberately"
        );
    }
}
