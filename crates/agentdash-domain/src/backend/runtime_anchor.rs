use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 运行期 backend identity 的唯一事实源。
///
/// 该值只能在 Lifecycle / AgentRun 创建或恢复边界生成；relay MCP、tool assembly、
/// extension invocation、VFS materialization 等下游运行期组件只能消费此 anchor，
/// 不得再从 session route、VFS mount 或在线 backend 列表重新选择 backend。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBackendAnchor {
    pub backend_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_binding_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_ref: Option<String>,
    pub source: RuntimeBackendAnchorSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeBackendAnchorSource {
    WorkspaceBinding,
    ExplicitBackend,
    RestoredAgentRun,
    System,
}

impl RuntimeBackendAnchorSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceBinding => "workspace_binding",
            Self::ExplicitBackend => "explicit_backend",
            Self::RestoredAgentRun => "restored_agent_run",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingRuntimeBackendAnchor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub component: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeBackendAnchorError {
    #[error(
        "runtime backend anchor missing: component={component}, session_id={session_id:?}, turn_id={turn_id:?}"
    )]
    Missing {
        component: String,
        session_id: Option<String>,
        turn_id: Option<String>,
    },
    #[error("runtime backend anchor invalid: field={field}, reason={reason}")]
    Invalid { field: &'static str, reason: String },
}

impl RuntimeBackendAnchor {
    pub fn new(
        backend_id: impl Into<String>,
        source: RuntimeBackendAnchorSource,
    ) -> Result<Self, RuntimeBackendAnchorError> {
        let backend_id = backend_id.into();
        if backend_id.trim().is_empty() {
            return Err(RuntimeBackendAnchorError::Invalid {
                field: "backend_id",
                reason: "backend_id 不能为空".to_string(),
            });
        }
        Ok(Self {
            backend_id: backend_id.trim().to_string(),
            workspace_id: None,
            workspace_binding_id: None,
            root_ref: None,
            source,
            source_detail: None,
        })
    }

    pub fn workspace_binding(
        backend_id: impl Into<String>,
        workspace_id: Uuid,
        workspace_binding_id: Uuid,
        root_ref: impl Into<String>,
    ) -> Result<Self, RuntimeBackendAnchorError> {
        let root_ref = root_ref.into();
        if root_ref.trim().is_empty() {
            return Err(RuntimeBackendAnchorError::Invalid {
                field: "root_ref",
                reason: "workspace binding root_ref 不能为空".to_string(),
            });
        }
        let mut anchor = Self::new(backend_id, RuntimeBackendAnchorSource::WorkspaceBinding)?;
        anchor.workspace_id = Some(workspace_id);
        anchor.workspace_binding_id = Some(workspace_binding_id);
        anchor.root_ref = Some(root_ref.trim().to_string());
        Ok(anchor)
    }

    pub fn with_workspace_id(mut self, workspace_id: Option<Uuid>) -> Self {
        self.workspace_id = workspace_id;
        self
    }

    pub fn with_workspace_binding_id(mut self, workspace_binding_id: Option<Uuid>) -> Self {
        self.workspace_binding_id = workspace_binding_id;
        self
    }

    pub fn with_root_ref(mut self, root_ref: Option<impl Into<String>>) -> Self {
        self.root_ref = root_ref
            .map(Into::into)
            .map(|value: String| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self
    }

    pub fn with_source_detail(mut self, source_detail: Option<impl Into<String>>) -> Self {
        self.source_detail = source_detail
            .map(Into::into)
            .map(|value: String| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self
    }

    pub fn backend_id(&self) -> &str {
        self.backend_id.as_str()
    }
}

impl MissingRuntimeBackendAnchor {
    pub fn new(
        component: impl Into<String>,
        session_id: Option<impl Into<String>>,
        turn_id: Option<impl Into<String>>,
    ) -> Self {
        Self {
            component: component.into(),
            session_id: session_id.map(Into::into),
            turn_id: turn_id.map(Into::into),
        }
    }

    pub fn into_error(self) -> RuntimeBackendAnchorError {
        RuntimeBackendAnchorError::Missing {
            component: self.component,
            session_id: self.session_id,
            turn_id: self.turn_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_binding_anchor_trims_and_records_source() {
        let workspace_id = Uuid::new_v4();
        let binding_id = Uuid::new_v4();

        let anchor = RuntimeBackendAnchor::workspace_binding(
            " backend-a ",
            workspace_id,
            binding_id,
            " /workspace ",
        )
        .expect("anchor");

        assert_eq!(anchor.backend_id(), "backend-a");
        assert_eq!(anchor.workspace_id, Some(workspace_id));
        assert_eq!(anchor.workspace_binding_id, Some(binding_id));
        assert_eq!(anchor.root_ref.as_deref(), Some("/workspace"));
        assert_eq!(anchor.source, RuntimeBackendAnchorSource::WorkspaceBinding);
    }

    #[test]
    fn anchor_rejects_empty_backend_id() {
        let error = RuntimeBackendAnchor::new(" ", RuntimeBackendAnchorSource::System)
            .expect_err("empty backend_id should fail");

        assert!(matches!(
            error,
            RuntimeBackendAnchorError::Invalid {
                field: "backend_id",
                ..
            }
        ));
    }

    #[test]
    fn missing_anchor_diagnostic_preserves_scope() {
        let error =
            MissingRuntimeBackendAnchor::new("relay_mcp", Some("session-1"), Some("turn-1"))
                .into_error();

        assert!(matches!(
            error,
            RuntimeBackendAnchorError::Missing {
                component,
                session_id,
                turn_id
            } if component == "relay_mcp"
                && session_id.as_deref() == Some("session-1")
                && turn_id.as_deref() == Some("turn-1")
        ));
    }
}
