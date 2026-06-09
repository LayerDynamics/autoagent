//! Tool input schema (M7) — a thin wrapper over a JSON Schema value.

use serde_json::{json, Map, Value};

#[derive(Debug, Clone)]
pub struct ToolSchema(Value);

impl ToolSchema {
    /// Build an object schema from `(field, json-type)` pairs.
    pub fn object(fields: &[(&str, &str)]) -> Self {
        let mut props = Map::new();
        for (name, ty) in fields {
            props.insert((*name).to_string(), json!({ "type": ty }));
        }
        ToolSchema(json!({ "type": "object", "properties": Value::Object(props) }))
    }

    pub fn from_value(v: Value) -> Self {
        ToolSchema(v)
    }

    pub fn as_value(&self) -> &Value {
        &self.0
    }

    /// Lightweight validation: required object fields are present with the right
    /// primitive JSON type (sufficient for the M7 tool ABI; a full JSON Schema
    /// validator is deferred).
    pub fn validate(&self, input: &Value) -> Result<(), String> {
        let props = match self.0.get("properties").and_then(|p| p.as_object()) {
            Some(p) => p,
            None => return Ok(()),
        };
        for (field, spec) in props {
            let expected = spec.get("type").and_then(|t| t.as_str()).unwrap_or("any");
            match input.get(field) {
                None => return Err(format!("missing field '{field}'")),
                Some(v) => {
                    let ok = match expected {
                        "string" => v.is_string(),
                        "number" => v.is_number(),
                        "boolean" => v.is_boolean(),
                        "object" => v.is_object(),
                        "array" => v.is_array(),
                        _ => true,
                    };
                    if !ok {
                        return Err(format!("field '{field}' must be {expected}"));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_required_fields() {
        let s = ToolSchema::object(&[("text", "string")]);
        assert!(s.validate(&json!({"text":"hi"})).is_ok());
        assert!(s.validate(&json!({})).is_err());
        assert!(s.validate(&json!({"text": 5})).is_err());
    }
}
