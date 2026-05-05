use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::backbone::BackboneEvent;

/// 事件来源标识。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub connector_id: String,
    pub connector_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor_id: Option<String>,
}

/// 追踪信息（与 turn / entry 关联）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TraceInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_index: Option<u32>,
}

/// 平台 envelope — 包裹每条 BackboneEvent，取代原 AgentDashMetaV1 的 source/trace 注入角色。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BackboneEnvelope {
    pub event: BackboneEvent,
    pub session_id: String,
    pub source: SourceInfo,
    pub trace: TraceInfo,
    pub observed_at: DateTime<Utc>,
}

impl BackboneEnvelope {
    pub fn new(event: BackboneEvent, session_id: impl Into<String>, source: SourceInfo) -> Self {
        Self {
            event,
            session_id: session_id.into(),
            source,
            trace: TraceInfo::default(),
            observed_at: Utc::now(),
        }
    }

    #[must_use]
    pub fn with_trace(mut self, trace: TraceInfo) -> Self {
        self.trace = trace;
        self
    }

    #[must_use]
    pub fn with_turn_id(mut self, turn_id: impl Into<String>) -> Self {
        self.trace.turn_id = Some(turn_id.into());
        self
    }

    #[must_use]
    pub fn with_entry_index(mut self, index: u32) -> Self {
        self.trace.entry_index = Some(index);
        self
    }
}
