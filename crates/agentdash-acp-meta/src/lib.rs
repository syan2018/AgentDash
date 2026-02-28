use agent_client_protocol::Meta;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

pub const AGENTDASH_META_NAMESPACE: &str = "agentdash";
pub const AGENTDASH_META_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AgentDashMetaV1 {
    pub v: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<AgentDashSourceV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<AgentDashEventV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<AgentDashTraceV1>,
}

impl Default for AgentDashMetaV1 {
    fn default() -> Self {
        Self {
            v: AGENTDASH_META_VERSION,
            source: None,
            event: None,
            trace: None,
        }
    }
}

impl AgentDashMetaV1 {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn source(mut self, source: impl Into<Option<AgentDashSourceV1>>) -> Self {
        self.source = source.into();
        self
    }

    #[must_use]
    pub fn event(mut self, event: impl Into<Option<AgentDashEventV1>>) -> Self {
        self.event = event.into();
        self
    }

    #[must_use]
    pub fn trace(mut self, trace: impl Into<Option<AgentDashTraceV1>>) -> Self {
        self.trace = trace.into();
        self
    }

    /// Convert to ACP `_meta` map containing `{ agentdash: <this> }`.
    pub fn to_acp_meta(&self) -> Meta {
        let mut meta = Meta::new();
        let value = serde_json::to_value(self).unwrap_or(Value::Null);
        meta.insert(AGENTDASH_META_NAMESPACE.to_string(), value);
        meta
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AgentDashSourceV1 {
    pub connector_id: String,
    pub connector_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

impl AgentDashSourceV1 {
    #[must_use]
    pub fn new(connector_id: impl Into<String>, connector_type: impl Into<String>) -> Self {
        Self {
            connector_id: connector_id.into(),
            connector_type: connector_type.into(),
            executor_id: None,
            variant: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AgentDashEventV1 {
    /// Free-form event type (e.g. "system_message", "error", "permission_denied").
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl AgentDashEventV1 {
    #[must_use]
    pub fn new(event_type: impl Into<String>) -> Self {
        Self {
            r#type: event_type.into(),
            severity: None,
            code: None,
            message: None,
            data: None,
        }
    }

    #[must_use]
    pub fn message(mut self, message: impl Into<Option<String>>) -> Self {
        self.message = message.into();
        self
    }

    #[must_use]
    pub fn data(mut self, data: impl Into<Option<Value>>) -> Self {
        self.data = data.into();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AgentDashTraceV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_index: Option<u32>,
}

impl AgentDashTraceV1 {
    #[must_use]
    pub fn new() -> Self {
        Self {
            session_event_id: None,
            parent_id: None,
            turn_id: None,
            entry_index: None,
        }
    }
}

/// Merge an AgentDash meta object into an existing ACP meta map.
pub fn merge_agentdash_meta(mut base: Option<Meta>, agentdash: &AgentDashMetaV1) -> Option<Meta> {
    let mut meta = base.take().unwrap_or_else(Meta::new);
    meta.insert(
        AGENTDASH_META_NAMESPACE.to_string(),
        serde_json::to_value(agentdash).unwrap_or(Value::Null),
    );
    Some(meta)
}

/// Parse AgentDash meta from ACP `_meta`.
pub fn parse_agentdash_meta(meta: &Meta) -> Option<AgentDashMetaV1> {
    meta.get(AGENTDASH_META_NAMESPACE)
        .and_then(|v| serde_json::from_value::<AgentDashMetaV1>(v.clone()).ok())
        .filter(|m| m.v == AGENTDASH_META_VERSION)
}

/// Convenience: wrap `{ agentdash: <AgentDashMetaV1> }` as a JSON object.
pub fn agentdash_meta_value(agentdash: &AgentDashMetaV1) -> Value {
    let mut map = Map::new();
    map.insert(
        AGENTDASH_META_NAMESPACE.to_string(),
        serde_json::to_value(agentdash).unwrap_or(Value::Null),
    );
    Value::Object(map)
}

