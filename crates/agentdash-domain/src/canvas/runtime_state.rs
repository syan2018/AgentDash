use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanvasRuntimeObservationStatus {
    Building,
    Ready,
    Error,
}

impl CanvasRuntimeObservationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }
}

impl TryFrom<&str> for CanvasRuntimeObservationStatus {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, <Self as TryFrom<&str>>::Error> {
        match value {
            "building" => Ok(Self::Building),
            "ready" => Ok(Self::Ready),
            "error" => Ok(Self::Error),
            other => Err(DomainError::InvalidConfig(format!(
                "canvas runtime observation status 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeViewport {
    pub width: i32,
    pub height: i32,
    pub device_pixel_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeDocumentState {
    pub root_empty: bool,
    pub body_text_preview: String,
    pub element_count: i32,
    pub focused_element: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeDiagnostic {
    pub level: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeObservation {
    pub observation_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub agent_run_canvas_ref: String,
    pub canvas_id: Uuid,
    pub canvas_mount_id: String,
    pub delivery_trace_ref: Option<String>,
    pub current_agent_frame_id: Option<Uuid>,
    pub frame_id: String,
    pub generation: i32,
    pub captured_at: DateTime<Utc>,
    pub status: CanvasRuntimeObservationStatus,
    pub message: Option<String>,
    pub viewport: CanvasRuntimeViewport,
    pub document: CanvasRuntimeDocumentState,
    pub diagnostics: Vec<CanvasRuntimeDiagnostic>,
    pub screenshot_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CanvasInteractionEvent {
    pub kind: String,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CanvasInteractionSnapshot {
    pub snapshot_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub agent_run_canvas_ref: String,
    pub canvas_id: Uuid,
    pub canvas_mount_id: String,
    pub delivery_trace_ref: Option<String>,
    pub current_agent_frame_id: Option<Uuid>,
    pub frame_id: String,
    pub updated_at: DateTime<Utc>,
    pub state: Value,
    pub recent_events: Vec<CanvasInteractionEvent>,
}

#[async_trait::async_trait]
pub trait CanvasRuntimeStateRepository: Send + Sync {
    async fn upsert_runtime_observation(
        &self,
        observation: CanvasRuntimeObservation,
    ) -> Result<CanvasRuntimeObservation, DomainError>;

    async fn latest_runtime_observation(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        canvas_mount_id: &str,
    ) -> Result<Option<CanvasRuntimeObservation>, DomainError>;

    async fn upsert_interaction_snapshot(
        &self,
        snapshot: CanvasInteractionSnapshot,
    ) -> Result<CanvasInteractionSnapshot, DomainError>;

    async fn latest_interaction_snapshot(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        canvas_mount_id: &str,
    ) -> Result<Option<CanvasInteractionSnapshot>, DomainError>;
}
