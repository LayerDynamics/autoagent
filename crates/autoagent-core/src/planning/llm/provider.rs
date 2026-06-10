//! LLM provider contract (M3, SPEC-1 FR-22). The model only *proposes* plans;
//! it never executes anything. The planner validates every provider output.

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct PlanRequest {
    pub objective: String,
    pub context: String,
    /// Optional JSON-Schema constraint on the provider's output (e.g. Ollama
    /// structured outputs). `None` = unconstrained free text. When set, a
    /// schema-aware provider forces the model to emit conforming JSON, which
    /// eliminates malformed-plan parse failures.
    pub format: Option<serde_json::Value>,
}

/// A single message in an agentic conversation (the read-edit-observe loop).
#[derive(Debug, Clone)]
pub struct Message {
    /// "system" | "user" | "assistant" | "tool".
    pub role: String,
    pub content: String,
}

/// A tool the model may call during the agentic loop (read-only context tools
/// like `read_file`/`grep`, exposed by the runtime). `parameters` is a JSON
/// Schema for the tool's arguments.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A model's request to invoke a tool.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// One step of the agentic loop: either the model asked to run tools, or it
/// produced its final answer (the plan JSON).
#[derive(Debug, Clone)]
pub enum Turn {
    ToolCalls(Vec<ToolCall>),
    Final(String),
}

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    /// Returns the model's raw text, expected to contain a JSON `Plan`.
    async fn complete(&self, req: &PlanRequest) -> Result<String>;

    /// Whether this provider can drive the agentic tool-use loop. Providers
    /// override `converse` AND return true here to opt in; the default is a
    /// one-shot provider that ignores tools.
    fn supports_tools(&self) -> bool {
        false
    }

    /// One turn of an agentic conversation. The default implementation has no
    /// native tool-use: it flattens the conversation into a single prompt and
    /// returns the model's reply as `Final` (so every existing provider keeps
    /// working unchanged). Tool-capable providers override this.
    async fn converse(&self, msgs: &[Message], _tools: &[ToolSpec]) -> Result<Turn> {
        let objective = msgs
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let context = msgs
            .iter()
            .map(|m| format!("[{}] {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        let text = self
            .complete(&PlanRequest {
                objective,
                context,
                format: None,
            })
            .await?;
        Ok(Turn::Final(text))
    }
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
                format: None,
            })
            .await
            .unwrap();
        let plan: crate::planning::plan::Plan = serde_json::from_str(&raw).unwrap();
        assert_eq!(plan.operations.len(), 1);
    }

    #[tokio::test]
    async fn default_converse_one_shots_to_final() {
        // A provider with no native tool-use still satisfies the agentic trait:
        // converse flattens the conversation and returns the reply as Final.
        let p = FakeProvider("PLAN_JSON".into());
        assert!(!p.supports_tools());
        let msgs = vec![
            Message {
                role: "system".into(),
                content: "you are a planner".into(),
            },
            Message {
                role: "user".into(),
                content: "add a cache".into(),
            },
        ];
        match p.converse(&msgs, &[]).await.unwrap() {
            Turn::Final(text) => assert_eq!(text, "PLAN_JSON"),
            Turn::ToolCalls(_) => panic!("default provider must not request tools"),
        }
    }
}
