mod exec;
mod lifecycle_gate;
mod mailbox;

pub(crate) use exec::{exec_item_from_terminal, terminal_belongs_to_scope};
pub(crate) use lifecycle_gate::{gate_belongs_to_scope, gate_item_from_gate};
pub(crate) use mailbox::{
    mailbox_belongs_to_scope, mailbox_item_from_message, mailbox_message_is_wait_relevant,
};

use serde_json::Value;

use super::types::WAIT_PREVIEW_CHARS;

fn payload_preview(payload: Option<&Value>) -> Option<String> {
    payload.and_then(|payload| {
        ["preview", "summary", "message", "title", "label"]
            .iter()
            .find_map(|key| payload_string(payload, key))
            .or_else(|| Some(bound_string(&payload.to_string(), WAIT_PREVIEW_CHARS)))
    })
}

fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| bound_string(value, WAIT_PREVIEW_CHARS))
}

fn bound_string(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let bounded = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{bounded}...")
    } else {
        bounded
    }
}
