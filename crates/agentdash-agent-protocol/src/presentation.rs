use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::codex_app_server_protocol as codex;
use crate::{AgentDashThreadItem, BackboneEnvelope, BackboneEvent, Turn};

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

/// A request-scoped interpretation of canonical conversation records.
///
/// This view owns no state and persists nothing. It centralizes the only valid fold from the
/// source-ordered protocol history into current turn and completed-item observations.
#[derive(Debug, Clone, Copy)]
pub struct CanonicalConversationView<'a> {
    records: &'a [CanonicalConversationRecord],
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CompletedConversationItem<'a> {
    pub thread_id: &'a str,
    pub turn_id: &'a str,
    pub item: &'a AgentDashThreadItem,
}

impl<'a> CanonicalConversationView<'a> {
    pub const fn new(records: &'a [CanonicalConversationRecord]) -> Self {
        Self { records }
    }

    pub const fn records(self) -> &'a [CanonicalConversationRecord] {
        self.records
    }

    pub fn latest_turn(self) -> Option<&'a Turn> {
        self.records
            .iter()
            .rev()
            .find_map(|record| match &record.presentation.envelope.event {
                BackboneEvent::TurnCompleted(notification) => Some(&notification.turn),
                BackboneEvent::TurnStarted(notification) => Some(&notification.turn),
                _ => None,
            })
    }

    pub fn active_turn(self) -> Option<&'a Turn> {
        self.latest_turn()
            .filter(|turn| turn.status == codex::TurnStatus::InProgress)
    }

    pub fn completed_turn(self, turn_id: Option<&str>) -> Option<&'a Turn> {
        self.records
            .iter()
            .rev()
            .find_map(|record| match &record.presentation.envelope.event {
                BackboneEvent::TurnCompleted(notification)
                    if turn_id.is_none_or(|turn_id| notification.turn.id == turn_id) =>
                {
                    Some(&notification.turn)
                }
                _ => None,
            })
    }

    pub fn completed_turns(self) -> impl Iterator<Item = &'a Turn> {
        self.records
            .iter()
            .filter_map(|record| match &record.presentation.envelope.event {
                BackboneEvent::TurnCompleted(notification) => Some(&notification.turn),
                _ => None,
            })
    }

    pub fn completed_items(self) -> impl Iterator<Item = CompletedConversationItem<'a>> {
        self.records
            .iter()
            .filter_map(|record| match &record.presentation.envelope.event {
                BackboneEvent::ItemCompleted(notification) => Some(CompletedConversationItem {
                    thread_id: &notification.thread_id,
                    turn_id: &notification.turn_id,
                    item: &notification.item,
                }),
                _ => None,
            })
    }

    pub fn completed_items_for_turn(
        self,
        turn_id: &'a str,
    ) -> impl Iterator<Item = CompletedConversationItem<'a>> {
        self.completed_items()
            .filter(move |completed| completed.turn_id == turn_id)
    }
}

#[cfg(test)]
mod tests {
    use crate::{BackboneEvent, SourceInfo};

    use super::*;

    #[test]
    fn record_round_trip_preserves_protected_event_body() {
        let event = BackboneEvent::UserInputSubmitted(crate::UserInputSubmittedNotification::new(
            "thread-1",
            "turn-1",
            "item-1",
            crate::UserInputSubmissionKind::Prompt,
            crate::UserInputSource::core_composer(),
            crate::text_user_input_blocks("hello"),
        ));
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
