use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const COMPANION_RESPOND_TOOL_NAME: &str = "companion_respond";
pub const COMPANION_PARENT_CHANNEL: &str = "companion.parent";
pub const COMPANION_CHILD_CHANNEL: &str = "companion.child";
pub const COMPANION_ACTION_CHANNEL: &str = "companion.action";

#[derive(Debug, Clone, PartialEq)]
pub struct CompanionReplyContract {
    pub route: CompanionReplyRoute,
    pub request_id: String,
    pub channel: String,
    pub aliases: Vec<String>,
    pub model_instruction: ModelReplyInstruction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanionReplyRoute {
    ChildDispatch,
    ParentRequestGate,
    PendingAction,
}

impl CompanionReplyContract {
    pub fn new(
        route: CompanionReplyRoute,
        request_id: impl Into<String>,
        channel: impl Into<String>,
        aliases: Vec<&'static str>,
        model_instruction: ModelReplyInstruction,
    ) -> Self {
        Self {
            route,
            request_id: request_id.into(),
            channel: channel.into(),
            aliases: aliases.into_iter().map(str::to_string).collect(),
            model_instruction,
        }
    }

    pub fn model_selector_label(&self) -> String {
        self.aliases
            .first()
            .map(|alias| format!("alias:{alias}"))
            .unwrap_or_else(|| format!("current:{}", self.channel))
    }

    pub fn available_selector_text(&self) -> String {
        let mut selectors = vec![format!("current:{}", self.channel)];
        selectors.extend(self.aliases.iter().map(|alias| format!("alias:{alias}")));
        selectors.join(", ")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompanionPayloadExpectation {
    pub expected_type: Option<String>,
    pub required_fields: Vec<String>,
    pub example_payload: serde_json::Value,
    pub repair_hint: Option<String>,
}

impl CompanionPayloadExpectation {
    pub fn completion() -> Self {
        Self {
            expected_type: Some("completion".to_string()),
            required_fields: vec![
                "type".to_string(),
                "status".to_string(),
                "summary".to_string(),
            ],
            example_payload: serde_json::json!({
                "type": "completion",
                "status": "completed",
                "summary": "..."
            }),
            repair_hint: Some(
                "payload must be a JSON object; registered response types are validated after tool schema parsing."
                    .to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelReplyInstruction {
    pub tool_name: String,
    pub minimal_arguments: serde_json::Value,
    pub reply_to: Option<ModelReplySelector>,
    pub payload_hint: CompanionPayloadExpectation,
    pub text_hint: String,
}

impl ModelReplyInstruction {
    pub fn completion_for_current_companion() -> Self {
        let payload_hint = CompanionPayloadExpectation::completion();
        Self::from_payload_expectation(payload_hint, None)
    }

    pub fn from_payload_expectation(
        payload_hint: CompanionPayloadExpectation,
        reply_to: Option<ModelReplySelector>,
    ) -> Self {
        let mut arguments = serde_json::Map::new();
        if let Some(selector) = reply_to.as_ref() {
            arguments.insert("reply_to".to_string(), selector.to_argument_value());
        }
        arguments.insert("payload".to_string(), payload_hint.example_payload.clone());

        Self {
            tool_name: COMPANION_RESPOND_TOOL_NAME.to_string(),
            minimal_arguments: serde_json::Value::Object(arguments),
            reply_to,
            payload_hint,
            text_hint: "Complete the assigned work, then call companion_respond with payload."
                .to_string(),
        }
    }

    pub fn with_reply_to(&self, reply_to: ModelReplySelector) -> Self {
        Self::from_payload_expectation(self.payload_hint.clone(), Some(reply_to))
    }

    pub fn minimal_arguments_json(&self) -> String {
        serde_json::to_string_pretty(&self.minimal_arguments)
            .unwrap_or_else(|_| self.minimal_arguments.to_string())
    }

    pub fn required_fields_text(&self) -> String {
        if self.payload_hint.required_fields.is_empty() {
            return "none".to_string();
        }
        self.payload_hint
            .required_fields
            .iter()
            .map(|field| format!("`{field}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn render_markdown_section(&self) -> String {
        let mut lines = vec![
            "## Reply Instruction".to_string(),
            self.text_hint.clone(),
            format!(
                "Call `{}` with this JSON argument:",
                self.tool_name.as_str()
            ),
            "```json".to_string(),
            self.minimal_arguments_json(),
            "```".to_string(),
            format!("Payload required fields: {}", self.required_fields_text()),
        ];

        if let Some(expected_type) = self.payload_hint.expected_type.as_deref() {
            lines.push(format!("Expected payload.type: `{expected_type}`"));
        }
        if let Some(repair_hint) = self.payload_hint.repair_hint.as_deref() {
            lines.push(format!("Repair hint: {repair_hint}"));
        }

        lines.join("\n")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelReplySelector {
    Current { channel: String },
    Alias { alias: String },
}

impl ModelReplySelector {
    pub fn current(channel: impl Into<String>) -> Self {
        Self::Current {
            channel: channel.into(),
        }
    }

    pub fn alias(alias: impl Into<String>) -> Self {
        Self::Alias {
            alias: alias.into(),
        }
    }

    pub fn to_argument_value(&self) -> serde_json::Value {
        match self {
            Self::Current { channel } => serde_json::json!({
                "kind": "current",
                "channel": channel
            }),
            Self::Alias { alias } => serde_json::json!({
                "kind": "alias",
                "alias": alias
            }),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Current { channel } => format!("current:{channel}"),
            Self::Alias { alias } => format!("alias:{alias}"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompanionReplySelectorParam {
    /// Select the current active companion reply target, optionally scoped by channel.
    Current {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
    },
    /// Select a short prompt-visible alias. Raw ids are not accepted.
    Alias { alias: String },
}

impl CompanionReplySelectorParam {
    pub fn received_label(&self) -> String {
        match self {
            Self::Current { channel } => channel
                .as_deref()
                .map(|value| format!("current:{value}"))
                .unwrap_or_else(|| "current".to_string()),
            Self::Alias { alias } => format!("alias:{alias}"),
        }
    }
}

pub fn normalize_reply_alias(alias: &str) -> Option<String> {
    let normalized = alias.trim().to_ascii_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

pub fn alias_is_raw_internal_ref(alias: &str) -> bool {
    let normalized = alias.trim();
    Uuid::parse_str(normalized).is_ok()
        || normalized.starts_with("dispatch-")
        || normalized.starts_with("gate-")
        || normalized.ends_with("_id")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_instruction_minimal_arguments_contains_payload_only() {
        let instruction = ModelReplyInstruction::completion_for_current_companion();

        assert!(instruction.minimal_arguments.get("payload").is_some());
        assert!(instruction.minimal_arguments.get("reply_to").is_none());
        assert!(!instruction.minimal_arguments_json().contains("dispatch_id"));
        assert!(!instruction.minimal_arguments_json().contains("gate_id"));
        assert!(!instruction.minimal_arguments_json().contains("run_id"));
        assert!(!instruction.minimal_arguments_json().contains("agent_id"));
        assert!(!instruction.minimal_arguments_json().contains("frame_id"));
        assert!(!instruction.minimal_arguments_json().contains("session_id"));
    }

    #[test]
    fn selector_instruction_includes_reply_to_only_when_explicit() {
        let instruction = ModelReplyInstruction::completion_for_current_companion()
            .with_reply_to(ModelReplySelector::alias("parent"));

        assert_eq!(
            instruction.minimal_arguments["reply_to"],
            serde_json::json!({ "kind": "alias", "alias": "parent" })
        );
    }
}
