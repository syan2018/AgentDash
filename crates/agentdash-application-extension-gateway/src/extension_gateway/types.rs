use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as SerdeError};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuntimeActionKey(String);

impl RuntimeActionKey {
    pub fn parse(value: impl Into<String>) -> Result<Self, RuntimeActionKeyError> {
        let value = value.into();
        validate_action_key(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuntimeActionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for RuntimeActionKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RuntimeActionKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        RuntimeActionKey::parse(value).map_err(D::Error::custom)
    }
}

impl FromStr for RuntimeActionKey {
    type Err = RuntimeActionKeyError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeActionKeyError {
    #[error("runtime action key 不能为空")]
    Empty,
    #[error("runtime action key 必须由小写字母、数字、下划线、短横线和点分段组成: {0}")]
    InvalidFormat(String),
}

fn validate_action_key(value: &str) -> Result<(), RuntimeActionKeyError> {
    if value.is_empty() {
        return Err(RuntimeActionKeyError::Empty);
    }

    let valid = value.split('.').all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    });

    if valid {
        Ok(())
    } else {
        Err(RuntimeActionKeyError::InvalidFormat(value.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActionKind {
    RuntimeThread,
    Setup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeActor {
    AgentRuntimeThread {
        runtime_thread_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_id: Option<String>,
    },
    UserCanvas {
        runtime_thread_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        canvas_id: Option<Uuid>,
    },
    WorkflowNode {
        runtime_thread_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<Uuid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_key: Option<String>,
    },
    RuntimeThreadUser {
        runtime_thread_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_id: Option<String>,
    },
    PlatformUser {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_id: Option<String>,
    },
    EnvironmentSetup {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
}

impl RuntimeActor {
    pub fn runtime_thread_id(&self) -> Option<&str> {
        match self {
            RuntimeActor::AgentRuntimeThread {
                runtime_thread_id, ..
            }
            | RuntimeActor::UserCanvas {
                runtime_thread_id, ..
            }
            | RuntimeActor::WorkflowNode {
                runtime_thread_id, ..
            }
            | RuntimeActor::RuntimeThreadUser {
                runtime_thread_id, ..
            } => Some(runtime_thread_id),
            RuntimeActor::PlatformUser { .. } | RuntimeActor::EnvironmentSetup { .. } => None,
        }
    }

    pub fn is_setup_actor(&self) -> bool {
        matches!(
            self,
            RuntimeActor::PlatformUser { .. } | RuntimeActor::EnvironmentSetup { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeContext {
    RuntimeThread {
        runtime_thread_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<Uuid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<Uuid>,
    },
    Setup {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<Uuid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<Uuid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        backend_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root_ref: Option<String>,
    },
}

impl RuntimeContext {
    pub fn action_kind(&self) -> RuntimeActionKind {
        match self {
            RuntimeContext::RuntimeThread { .. } => RuntimeActionKind::RuntimeThread,
            RuntimeContext::Setup { .. } => RuntimeActionKind::Setup,
        }
    }

    pub fn runtime_thread_id(&self) -> Option<&str> {
        match self {
            RuntimeContext::RuntimeThread {
                runtime_thread_id, ..
            } => Some(runtime_thread_id),
            RuntimeContext::Setup { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeTarget {
    CurrentRuntimeThread,
    Backend { backend_id: String },
    Workspace { workspace_id: Uuid },
    McpServer { name: String },
    Http { url: String },
    Custom { kind: String, id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePolicy {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub allow_background: bool,
}

impl Default for RuntimePolicy {
    fn default() -> Self {
        Self {
            required_capabilities: Vec::new(),
            timeout_ms: Some(30_000),
            allow_background: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTrace {
    pub trace_id: String,
    pub invocation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_trace_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl RuntimeTrace {
    pub fn new() -> Self {
        Self::with_parent(None)
    }

    pub fn with_parent(parent_trace_id: Option<String>) -> Self {
        Self {
            trace_id: format!("trace-{}", Uuid::new_v4().simple()),
            invocation_id: format!("rtinv-{}", Uuid::new_v4().simple()),
            parent_trace_id,
            created_at: Utc::now(),
        }
    }
}

impl Default for RuntimeTrace {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeInvocationRequest {
    pub action_key: RuntimeActionKey,
    pub actor: RuntimeActor,
    pub context: RuntimeContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<RuntimeTarget>,
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub policy: RuntimePolicy,
    #[serde(default)]
    pub trace: RuntimeTrace,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

impl RuntimeInvocationRequest {
    pub fn new(
        action_key: RuntimeActionKey,
        actor: RuntimeActor,
        context: RuntimeContext,
        input: Value,
    ) -> Self {
        Self {
            action_key,
            actor,
            context,
            target: None,
            input,
            policy: RuntimePolicy::default(),
            trace: RuntimeTrace::new(),
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeInvocationOutput {
    #[serde(default)]
    pub output: Value,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

impl RuntimeInvocationOutput {
    pub fn new(output: Value) -> Self {
        Self {
            output,
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeInvocationResult {
    pub action_key: RuntimeActionKey,
    pub trace: RuntimeTrace,
    pub output: RuntimeInvocationOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeActionDescriptor {
    pub action_key: RuntimeActionKey,
    pub kind: RuntimeActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub default_policy: RuntimePolicy,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

impl RuntimeActionDescriptor {
    pub fn new(action_key: RuntimeActionKey, kind: RuntimeActionKind) -> Self {
        Self {
            action_key,
            kind,
            description: None,
            input_schema: None,
            output_schema: None,
            default_policy: RuntimePolicy::default(),
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeSurface {
    pub context: RuntimeContext,
    pub actions: Vec<RuntimeActionDescriptor>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_key_parse_rejects_invalid_format() {
        assert!(RuntimeActionKey::parse("workspace.detect").is_ok());
        assert!(RuntimeActionKey::parse("Workspace.detect").is_err());
        assert!(RuntimeActionKey::parse("workspace..detect").is_err());
    }

    #[test]
    fn action_key_deserialize_uses_validation() {
        let err = serde_json::from_str::<RuntimeActionKey>(r#""workspace..detect""#)
            .expect_err("invalid key should fail deserialization");
        assert!(
            err.to_string().contains("runtime action key"),
            "unexpected error: {err}"
        );
    }
}
