//! LLM provider contract (M3, SPEC-1 FR-22). The model only *proposes* plans;
//! it never executes anything. The planner validates every provider output.

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct PlanRequest {
    pub objective: String,
    pub context: String,
}

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    /// Returns the model's raw text, expected to contain a JSON `Plan`.
    async fn complete(&self, req: &PlanRequest) -> Result<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProvider(String);

    #[async_trait::async_trait]
    impl LlmProvider for FakeProvider {
        fn name(&self) -> &str {
            "fake"
        }
        async fn complete(&self, _req: &PlanRequest) -> Result<String> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn provider_returns_parseable_plan_json() {
        let p = FakeProvider(
            r#"{"objective":"o","summary":"s","files_to_read":[],
          "files_to_create":[],"files_to_modify":[],
          "operations":[{"kind":"Create","path":"crates/x.rs","destination_path":null,
            "reason":"r","before_hash":null,"after_hash":null,"content":"x"}],
          "validation_commands":[],"risks":[],"rollback_strategy":"snapshot"}"#
                .into(),
        );
        let raw = p
            .complete(&PlanRequest {
                objective: "o".into(),
                context: "ctx".into(),
            })
            .await
            .unwrap();
        let plan: crate::planning::plan::Plan = serde_json::from_str(&raw).unwrap();
        assert_eq!(plan.operations.len(), 1);
    }
}
