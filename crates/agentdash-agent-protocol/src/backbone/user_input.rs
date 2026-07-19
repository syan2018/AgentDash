use crate::codex_app_server_protocol as codex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// AgentDash canonical user-input unit.
pub type UserInputBlock = codex::UserInput;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserInputSubmissionKind {
    Prompt,
    Steer,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserInputSource {
    pub namespace: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_ref: Option<String>,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    pub display_label_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl UserInputSource {
    pub fn new(
        namespace: impl Into<String>,
        kind: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let namespace = namespace.into();
        let kind = kind.into();
        Self {
            display_label_key: format!("mailbox.source.{namespace}.{kind}"),
            namespace,
            kind,
            source_ref: None,
            correlation_ref: None,
            actor: actor.into(),
            route: None,
            metadata: None,
        }
    }

    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }

    pub fn core_composer() -> Self {
        Self::new("core", "composer", "user")
    }

    pub fn companion_parent_resume() -> Self {
        Self::new("companion", "parent_resume", "agent").with_route("parent")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct UserInputSubmittedNotification {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub submission_kind: UserInputSubmissionKind,
    pub source: UserInputSource,
    pub content: Vec<codex::UserInput>,
}

impl UserInputSubmittedNotification {
    pub fn new(
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
        item_id: impl Into<String>,
        submission_kind: UserInputSubmissionKind,
        source: UserInputSource,
        content: Vec<codex::UserInput>,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            item_id: item_id.into(),
            submission_kind,
            source,
            content,
        }
    }
}

pub fn user_input_text(input: &codex::UserInput) -> Option<&str> {
    match input {
        codex::UserInput::Text { text, .. } => Some(text.as_str()),
        _ => None,
    }
}

pub fn text_user_input_block(text: impl Into<String>) -> UserInputBlock {
    codex::UserInput::Text {
        text: text.into(),
        text_elements: Vec::new(),
    }
}

pub fn text_user_input_blocks(text: impl Into<String>) -> Vec<UserInputBlock> {
    vec![text_user_input_block(text)]
}

#[derive(Debug, Error)]
pub enum UserInputConversionError {
    #[error("Codex UserInput 中没有可投递文本")]
    EmptyTextInput,
}

/// Canonical user input to display-only text projection.
pub fn codex_user_input_to_text(
    input: &[codex::UserInput],
) -> Result<String, UserInputConversionError> {
    let text = input
        .iter()
        .map(|item| match item {
            codex::UserInput::Text { text, .. } => text.as_str(),
            codex::UserInput::Image { url, .. } => url.as_str(),
            codex::UserInput::LocalImage { path, .. } => path.as_str(),
            codex::UserInput::Skill { name, .. } => name.as_str(),
            codex::UserInput::Mention { name, .. } => name.as_str(),
        })
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        return Err(UserInputConversionError::EmptyTextInput);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_input_summary_preserves_order() {
        let input = vec![
            text_user_input_block("hello"),
            codex::UserInput::Mention {
                name: "main.rs".to_string(),
                path: "file://src/main.rs".to_string(),
            },
        ];

        assert_eq!(
            codex_user_input_to_text(&input).expect("summarize input"),
            "hello\nmain.rs"
        );
    }

    #[test]
    fn legacy_envelope_requires_explicit_source_backfill() {
        let envelope = crate::BackboneEnvelope::new(
            crate::BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
                "thread-1",
                "turn-1",
                "item-1",
                UserInputSubmissionKind::Prompt,
                UserInputSource::core_composer(),
                vec![text_user_input_block("hello")],
            )),
            "session-1",
            crate::SourceInfo {
                connector_id: "test".to_string(),
                connector_type: "test".to_string(),
                executor_id: None,
            },
        );
        let mut value = serde_json::to_value(&envelope).expect("serialize envelope");
        value["event"]["payload"]
            .as_object_mut()
            .expect("input payload")
            .remove("source");

        assert!(serde_json::from_value::<crate::BackboneEnvelope>(value).is_err());
    }
}
