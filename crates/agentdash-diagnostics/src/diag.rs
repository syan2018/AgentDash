//! 通用诊断构造工具。
//!
//! [`crate::diag!`] 负责普通平台过程诊断；[`crate::diag_error!`] 负责标准错误诊断。
//! 本模块提供错误诊断的 operation / stage / 关联字段上下文，由 `diag_error!`
//! 统一组装 detail 与 JSON context。

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticErrorContext {
    operation: String,
    stage: String,
    fields: Vec<(String, String)>,
}

impl DiagnosticErrorContext {
    pub fn new(operation: impl Into<String>, stage: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            stage: stage.into(),
            fields: Vec::new(),
        }
    }

    pub fn with_field(mut self, key: impl Into<String>, value: impl std::fmt::Display) -> Self {
        self.fields.push((key.into(), value.to_string()));
        self
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn stage(&self) -> &str {
        &self.stage
    }

    pub fn detail<E>(&self, error: &E) -> String
    where
        E: std::fmt::Debug + std::fmt::Display,
    {
        let mut parts = Vec::with_capacity(self.fields.len() + 3);
        parts.push(format!("operation={}", self.operation));
        parts.push(format!("stage={}", self.stage));
        parts.extend(
            self.fields
                .iter()
                .map(|(key, value)| format!("{key}={value}")),
        );
        parts.push(format!("error={error:?}"));
        format!("diagnostic failure: {}", parts.join(", "))
    }

    pub fn context_json(&self) -> Value {
        let mut map = Map::new();
        map.insert(
            "operation".to_string(),
            Value::String(self.operation.clone()),
        );
        map.insert("stage".to_string(), Value::String(self.stage.clone()));
        for (key, value) in &self.fields {
            map.insert(key.clone(), Value::String(value.clone()));
        }
        Value::Object(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_includes_operation_stage_fields_and_debug_error() {
        let context = DiagnosticErrorContext::new("agent_run.fork", "materialization")
            .with_field("run_id", "run-1")
            .with_field("client_command_id", "cmd-1")
            .with_field("fork_point", "turn-1:2");
        let error = std::io::Error::new(std::io::ErrorKind::Other, "database exploded");

        let detail = context.detail(&error);

        assert!(detail.contains("operation=agent_run.fork"));
        assert!(detail.contains("stage=materialization"));
        assert!(detail.contains("run_id=run-1"));
        assert!(detail.contains("client_command_id=cmd-1"));
        assert!(detail.contains("fork_point=turn-1:2"));
        assert!(detail.contains("database exploded"));
    }

    #[test]
    fn context_json_preserves_standard_fields() {
        let context = DiagnosticErrorContext::new("agent_run.fork", "route")
            .with_field("run_id", "run-1")
            .with_field("agent_id", "agent-1");

        let json = context.context_json();

        assert_eq!(json["operation"], "agent_run.fork");
        assert_eq!(json["stage"], "route");
        assert_eq!(json["run_id"], "run-1");
        assert_eq!(json["agent_id"], "agent-1");
    }
}
