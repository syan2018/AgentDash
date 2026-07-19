use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::BackboneEnvelope;

/// Presentation durability is explicit producer evidence and is never inferred
/// from a Runtime cursor or transport replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum PresentationDurability {
    Durable,
    Ephemeral,
}

/// One immutable, AgentDash-owned App Server Protocol-shaped presentation body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanonicalConversationPresentation {
    pub durability: PresentationDurability,
    pub envelope: BackboneEnvelope,
}

impl CanonicalConversationPresentation {
    pub const fn new(durability: PresentationDurability, envelope: BackboneEnvelope) -> Self {
        Self {
            durability,
            envelope,
        }
    }
}

/// Stable source history record. Vector order is the observable presentation order;
/// `presentation_id` remains stable across snapshot hydration and ordered changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanonicalConversationRecord {
    pub presentation_id: String,
    pub presentation: CanonicalConversationPresentation,
}

impl CanonicalConversationRecord {
    pub fn new(
        presentation_id: impl Into<String>,
        presentation: CanonicalConversationPresentation,
    ) -> Self {
        Self {
            presentation_id: presentation_id.into(),
            presentation,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{BackboneEvent, SourceInfo};

    use super::*;

    #[test]
    fn record_round_trip_preserves_protected_event_body() {
        let event = BackboneEvent::UserInputSubmitted(
            crate::UserInputSubmittedNotification::new(
                "thread-1",
                "turn-1",
                "item-1",
                crate::UserInputSubmissionKind::Prompt,
                crate::UserInputSource::core_composer(),
                crate::text_user_input_blocks("hello"),
            ),
        );
        let record = CanonicalConversationRecord::new(
            "native:thread-1:1",
            CanonicalConversationPresentation::new(
                PresentationDurability::Durable,
                BackboneEnvelope::new(
                    event.clone(),
                    "thread-1",
                    SourceInfo {
                        connector_id: "native".to_owned(),
                        connector_type: "native".to_owned(),
                        executor_id: None,
                    },
                ),
            ),
        );

        let value = serde_json::to_value(&record).expect("serialize record");
        let decoded: CanonicalConversationRecord =
            serde_json::from_value(value).expect("deserialize record");
        assert_eq!(decoded.presentation.envelope.event, event);
        assert_eq!(decoded, record);
    }
}
