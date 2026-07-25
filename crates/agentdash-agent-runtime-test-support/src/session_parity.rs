//! Strict presentation-event parity support.
//!
//! The normalizers in this module understand only the two explicitly allowed
//! transport placements. They never traverse or rewrite the protected event
//! body, so JSON object fields, explicit nulls, array order, scalar types, IDs,
//! and timestamps all participate in equality.

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationDurability {
    Durable,
    Ephemeral,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedPresentationEvent {
    pub durability: PresentationDurability,
    pub event: Value,
}

#[derive(Debug, Error, PartialEq)]
pub enum SessionParityError {
    #[error("session parity frame must be a JSON object")]
    FrameMustBeObject,
    #[error("main session frame does not match the allowlisted typed wrapper")]
    InvalidMainSessionWrapper,
    #[error("current session frame does not match the allowlisted typed wrapper")]
    InvalidCurrentPresentationWrapper,
    #[error("unsupported main NDJSON control or event frame type: {0}")]
    UnsupportedMainFrame(String),
    #[error("presentation event count differs: main={main}, current={current}")]
    EventCount { main: usize, current: usize },
    #[error("presentation durability differs at index {index}: main={main:?}, current={current:?}")]
    Durability {
        index: usize,
        main: PresentationDurability,
        current: PresentationDurability,
    },
    #[error("protected presentation body differs at index {index}")]
    ProtectedBody {
        index: usize,
        main: Value,
        current: Value,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct MainSourceInfo {
    connector_id: String,
    connector_type: String,
    executor_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct MainTraceInfo {
    turn_id: Option<String>,
    entry_index: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[allow(dead_code)]
struct MainNotification {
    session_id: String,
    source: MainSourceInfo,
    trace: MainTraceInfo,
    observed_at: String,
    event: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct MainSessionFrame {
    #[serde(rename = "type")]
    frame_type: Option<String>,
    session_id: String,
    event_seq: u64,
    occurred_at_ms: i64,
    committed_at_ms: i64,
    session_update_type: String,
    turn_id: Option<String>,
    entry_index: Option<u32>,
    tool_call_id: Option<String>,
    notification: MainNotification,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct CurrentPresentationFrame {
    runtime_thread_id: String,
    runtime_revision: u64,
    durable_sequence: u64,
    presentation_event: Value,
}

/// Unwraps the main `SessionEventResponse.notification.event` placement.
///
/// Wrapper fields are deserialized and discarded as a unit. The event itself
/// is moved out without field-level normalization.
pub fn normalize_main_session_event(
    frame: Value,
    durability: PresentationDurability,
) -> Result<NormalizedPresentationEvent, SessionParityError> {
    if !frame.is_object() {
        return Err(SessionParityError::FrameMustBeObject);
    }
    let frame: MainSessionFrame =
        serde_json::from_value(frame).map_err(|_| SessionParityError::InvalidMainSessionWrapper)?;
    Ok(NormalizedPresentationEvent {
        durability,
        event: frame.notification.event,
    })
}

/// Unwraps the current immutable carrier's `presentation_event` placement.
///
/// This is intentionally a separate typed entry point: accepting both shapes
/// in one permissive parser would hide placement regressions.
pub fn normalize_current_presentation_event(
    frame: Value,
    durability: PresentationDurability,
) -> Result<NormalizedPresentationEvent, SessionParityError> {
    if !frame.is_object() {
        return Err(SessionParityError::FrameMustBeObject);
    }
    let frame: CurrentPresentationFrame = serde_json::from_value(frame)
        .map_err(|_| SessionParityError::InvalidCurrentPresentationWrapper)?;
    Ok(NormalizedPresentationEvent {
        durability,
        event: frame.presentation_event,
    })
}

/// Normalizes a main NDJSON event frame and keeps control frames on a separate
/// channel by returning `None` for `connected` and `heartbeat`.
pub fn normalize_main_ndjson_frame(
    frame: Value,
) -> Result<Option<NormalizedPresentationEvent>, SessionParityError> {
    let Some(frame_type) = frame.get("type").and_then(Value::as_str) else {
        return Err(SessionParityError::UnsupportedMainFrame(
            "<missing>".to_string(),
        ));
    };
    match frame_type {
        "event" => normalize_main_session_event(frame, PresentationDurability::Durable).map(Some),
        "ephemeral_event" => {
            normalize_main_session_event(frame, PresentationDurability::Ephemeral).map(Some)
        }
        "connected" | "heartbeat" => Ok(None),
        other => Err(SessionParityError::UnsupportedMainFrame(other.to_string())),
    }
}

/// Compares ordered presentation streams without an ignore list or semantic
/// coercion. `serde_json::Value` equality preserves null-vs-omitted, scalar
/// types, and array order.
pub fn compare_ordered_presentation_events(
    main: &[NormalizedPresentationEvent],
    current: &[NormalizedPresentationEvent],
) -> Result<(), SessionParityError> {
    if main.len() != current.len() {
        return Err(SessionParityError::EventCount {
            main: main.len(),
            current: current.len(),
        });
    }
    for (index, (main, current)) in main.iter().zip(current).enumerate() {
        if main.durability != current.durability {
            return Err(SessionParityError::Durability {
                index,
                main: main.durability,
                current: current.durability,
            });
        }
        if main.event != current.event {
            return Err(SessionParityError::ProtectedBody {
                index,
                main: main.event.clone(),
                current: current.event.clone(),
            });
        }
    }
    Ok(())
}
