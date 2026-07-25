use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentEffectIdentity, AgentItemId, AgentProfileDigest,
    AgentServiceInstanceId, AgentSourceCoordinate, AgentSurfaceDigest, AgentSurfaceRevision,
    AgentToolName, AgentToolResult, AgentTurnId,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeToolEffect {
    ReadOnly,
    ProductMutation,
    VfsMutation,
    LocalProcess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeToolPermission {
    ProductRead,
    ProductWrite,
    VfsRead,
    VfsWrite,
    ProcessExecute,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeToolProvenance {
    pub capability_key: String,
    pub source: String,
    pub tool_path: String,
    pub context_usage_kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeToolDefinition {
    pub name: AgentToolName,
    pub description: String,
    pub parameters_schema: Value,
    pub provenance: RuntimeToolProvenance,
    pub protocol_projector: agentdash_agent_protocol::ToolProtocolProjector,
    pub permission: RuntimeToolPermission,
    pub effect: RuntimeToolEffect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeToolResolvedContext {
    pub runtime_thread_id: RuntimeThreadId,
    pub binding_generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub service_instance_id: AgentServiceInstanceId,
    pub profile_digest: AgentProfileDigest,
    pub bound_surface_revision: AgentSurfaceRevision,
    pub bound_surface_digest: AgentSurfaceDigest,
    pub applied_surface_revision: AgentSurfaceRevision,
    pub applied_surface_digest: AgentSurfaceDigest,
    pub turn_id: AgentTurnId,
    pub item_id: Option<AgentItemId>,
    pub effect_id: AgentEffectIdentity,
    pub callback_idempotency_key: String,
    pub deadline_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeToolAuthorizationRequest {
    pub context: RuntimeToolResolvedContext,
    pub definition: RuntimeToolDefinition,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeToolAuthorizationGrant {
    pub permission: RuntimeToolPermission,
    pub effect: RuntimeToolEffect,
    pub target: RuntimeToolProductTarget,
    pub applied_surface: RuntimeToolAppliedSurfaceEvidence,
    pub resources: RuntimeToolResourceGrant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeToolProductTarget {
    pub project_id: String,
    pub run_id: String,
    pub agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeToolAppliedSurfaceEvidence {
    pub agent_surface_revision: u64,
    pub agent_surface_digest: String,
    pub vfs_digest: String,
    pub vfs_provenance: RuntimeToolProvenanceEvidence,
    pub task_digest: String,
    pub product_binding_digest: String,
    pub host_binding_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeToolProvenanceEvidence {
    pub source_kind: String,
    pub source_id: String,
    pub source_revision: u64,
    pub projection_revision: u64,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeToolResourceGrant {
    Product,
    Task(RuntimeTaskExecutionGrant),
    Vfs(RuntimeVfsExecutionGrant),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTaskExecutionGrant {
    pub scope: RuntimeTaskExecutionScope,
    pub plan_digest: String,
    pub operations: Vec<RuntimeTaskGrantedOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTaskExecutionScope {
    Project { project_id: String },
    Task { project_id: String, task_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeTaskGrantedOperation {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeVfsExecutionGrant {
    pub default_mount_id: Option<String>,
    pub mounts: Vec<RuntimeVfsMountGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeVfsMountGrant {
    pub id: String,
    pub provider: String,
    pub backend_id: String,
    pub root_ref: String,
    pub display_name: String,
    pub metadata: Value,
    pub operations: Vec<RuntimeVfsGrantedOperation>,
    pub path_scopes: Vec<RuntimeVfsPathGrant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeVfsGrantedOperation {
    Read,
    List,
    Search,
    Write,
    Execute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeVfsPathGrant {
    All,
    Prefix(String),
    Exact(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeToolInvocation {
    pub context: RuntimeToolResolvedContext,
    pub tool: AgentToolName,
    pub arguments: Value,
    pub grant: RuntimeToolAuthorizationGrant,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RuntimeToolBrokerError {
    #[error("runtime tool catalog must contain at least one executor")]
    EmptyCatalog,
    #[error("runtime tool `{0}` is not registered")]
    UnknownTool(String),
    #[error("runtime tool `{0}` is registered more than once")]
    DuplicateTool(String),
    #[error("runtime tool `{tool}` requires permission {required:?}, received {actual:?}")]
    PermissionDenied {
        tool: String,
        required: RuntimeToolPermission,
        actual: RuntimeToolPermission,
    },
    #[error("runtime tool `{tool}` requires effect {required:?}, received {actual:?}")]
    EffectMismatch {
        tool: String,
        required: RuntimeToolEffect,
        actual: RuntimeToolEffect,
    },
    #[error("runtime tool authorization denied ({code}): {message}")]
    AuthorizationDenied { code: String, message: String },
}

#[async_trait]
pub trait RuntimeToolAuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        request: RuntimeToolAuthorizationRequest,
    ) -> Result<RuntimeToolAuthorizationGrant, RuntimeToolBrokerError>;
}

#[async_trait]
pub trait RuntimeToolExecutor: Send + Sync {
    fn definition(&self) -> RuntimeToolDefinition;

    async fn execute(&self, invocation: RuntimeToolInvocation) -> AgentToolResult;
}

pub struct PlatformToolBroker {
    executors: BTreeMap<AgentToolName, Arc<dyn RuntimeToolExecutor>>,
    runtime_executors:
        RwLock<BTreeMap<RuntimeThreadId, BTreeMap<AgentToolName, Arc<dyn RuntimeToolExecutor>>>>,
    authorization: Arc<dyn RuntimeToolAuthorizationPort>,
}

impl PlatformToolBroker {
    pub fn new(
        executors: impl IntoIterator<Item = Arc<dyn RuntimeToolExecutor>>,
        authorization: Arc<dyn RuntimeToolAuthorizationPort>,
    ) -> Result<Self, RuntimeToolBrokerError> {
        let mut catalog = BTreeMap::new();
        for executor in executors {
            let name = executor.definition().name;
            if catalog.insert(name.clone(), executor).is_some() {
                return Err(RuntimeToolBrokerError::DuplicateTool(name.to_string()));
            }
        }
        if catalog.is_empty() {
            return Err(RuntimeToolBrokerError::EmptyCatalog);
        }
        Ok(Self {
            executors: catalog,
            runtime_executors: RwLock::new(BTreeMap::new()),
            authorization,
        })
    }

    pub fn definition(&self, name: &AgentToolName) -> Option<RuntimeToolDefinition> {
        self.executors
            .get(name)
            .map(|executor| executor.definition())
    }

    pub fn definitions(&self) -> Vec<RuntimeToolDefinition> {
        self.executors
            .values()
            .map(|executor| executor.definition())
            .collect()
    }

    /// Binds the exact dynamic tool catalog compiled for one immutable Runtime target.
    ///
    /// Static Product/VFS tools remain process registrations. MCP and other surface-scoped tools
    /// live here so the compiler and callback executor consume the same definitions and handles.
    pub async fn bind_runtime_catalog(
        &self,
        runtime_thread_id: RuntimeThreadId,
        executors: impl IntoIterator<Item = Arc<dyn RuntimeToolExecutor>>,
    ) -> Result<Vec<RuntimeToolDefinition>, RuntimeToolBrokerError> {
        let mut catalog = BTreeMap::new();
        for executor in executors {
            let definition = executor.definition();
            if self.executors.contains_key(&definition.name)
                || catalog.insert(definition.name.clone(), executor).is_some()
            {
                return Err(RuntimeToolBrokerError::DuplicateTool(
                    definition.name.to_string(),
                ));
            }
        }
        let definitions = catalog
            .values()
            .map(|executor| executor.definition())
            .collect::<Vec<_>>();
        let mut runtime_executors = self.runtime_executors.write().await;
        if let Some(existing) = runtime_executors.get(&runtime_thread_id) {
            let existing_definitions = existing
                .values()
                .map(|executor| executor.definition())
                .collect::<Vec<_>>();
            if existing_definitions == definitions {
                return Ok(definitions);
            }
            return Err(RuntimeToolBrokerError::DuplicateTool(format!(
                "runtime catalog for {runtime_thread_id}"
            )));
        }
        runtime_executors.insert(runtime_thread_id, catalog);
        Ok(definitions)
    }

    pub async fn runtime_definitions(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Vec<RuntimeToolDefinition> {
        self.runtime_executors
            .read()
            .await
            .get(runtime_thread_id)
            .into_iter()
            .flat_map(BTreeMap::values)
            .map(|executor| executor.definition())
            .collect()
    }

    pub async fn invoke(
        &self,
        context: RuntimeToolResolvedContext,
        tool: AgentToolName,
        arguments: Value,
    ) -> Result<AgentToolResult, RuntimeToolBrokerError> {
        let scoped = self
            .runtime_executors
            .read()
            .await
            .get(&context.runtime_thread_id)
            .and_then(|catalog| catalog.get(&tool))
            .cloned();
        let executor = scoped
            .or_else(|| self.executors.get(&tool).cloned())
            .ok_or_else(|| RuntimeToolBrokerError::UnknownTool(tool.to_string()))?;
        let definition = executor.definition();
        let grant = self
            .authorization
            .authorize(RuntimeToolAuthorizationRequest {
                context: context.clone(),
                definition: definition.clone(),
                arguments: arguments.clone(),
            })
            .await?;
        if definition.permission != grant.permission {
            return Err(RuntimeToolBrokerError::PermissionDenied {
                tool: tool.to_string(),
                required: definition.permission,
                actual: grant.permission,
            });
        }
        if definition.effect != grant.effect {
            return Err(RuntimeToolBrokerError::EffectMismatch {
                tool: tool.to_string(),
                required: definition.effect,
                actual: grant.effect,
            });
        }
        Ok(executor
            .execute(RuntimeToolInvocation {
                context,
                tool,
                arguments,
                grant,
            })
            .await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Allow;

    #[async_trait]
    impl RuntimeToolAuthorizationPort for Allow {
        async fn authorize(
            &self,
            request: RuntimeToolAuthorizationRequest,
        ) -> Result<RuntimeToolAuthorizationGrant, RuntimeToolBrokerError> {
            Ok(RuntimeToolAuthorizationGrant {
                permission: request.definition.permission,
                effect: request.definition.effect,
                target: RuntimeToolProductTarget {
                    project_id: "project-test".into(),
                    run_id: "run-test".into(),
                    agent_id: "agent-test".into(),
                },
                applied_surface: RuntimeToolAppliedSurfaceEvidence {
                    agent_surface_revision: 1,
                    agent_surface_digest: "surface-test".into(),
                    vfs_digest: "vfs-test".into(),
                    vfs_provenance: provenance(),
                    task_digest: "task-test".into(),
                    product_binding_digest: "binding-test".into(),
                    host_binding_generation: 1,
                },
                resources: RuntimeToolResourceGrant::Vfs(RuntimeVfsExecutionGrant {
                    default_mount_id: None,
                    mounts: Vec::new(),
                }),
            })
        }
    }

    struct Deny;

    #[async_trait]
    impl RuntimeToolAuthorizationPort for Deny {
        async fn authorize(
            &self,
            _request: RuntimeToolAuthorizationRequest,
        ) -> Result<RuntimeToolAuthorizationGrant, RuntimeToolBrokerError> {
            Err(RuntimeToolBrokerError::AuthorizationDenied {
                code: "missing_product_grant".into(),
                message: "runtime thread has no Product authorization grant".into(),
            })
        }
    }

    struct MountsList;

    #[async_trait]
    impl RuntimeToolExecutor for MountsList {
        fn definition(&self) -> RuntimeToolDefinition {
            RuntimeToolDefinition {
                name: AgentToolName::new("mounts_list").unwrap(),
                description: "List the VFS mounts bound to this runtime surface.".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "additionalProperties": false
                }),
                provenance: RuntimeToolProvenance {
                    capability_key: "file_read".into(),
                    source: "platform:read".into(),
                    tool_path: "file_read::mounts_list".into(),
                    context_usage_kind: "system_tools".into(),
                },
                protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::Dynamic,
                permission: RuntimeToolPermission::VfsRead,
                effect: RuntimeToolEffect::ReadOnly,
            }
        }

        async fn execute(&self, _invocation: RuntimeToolInvocation) -> AgentToolResult {
            AgentToolResult::Completed {
                output: serde_json::json!({"mounts": ["main"]}),
            }
        }
    }

    struct DynamicTool;

    #[async_trait]
    impl RuntimeToolExecutor for DynamicTool {
        fn definition(&self) -> RuntimeToolDefinition {
            RuntimeToolDefinition {
                name: AgentToolName::new("mcp_docs_search").unwrap(),
                description: "Search docs".into(),
                parameters_schema: serde_json::json!({"type": "object"}),
                provenance: RuntimeToolProvenance {
                    capability_key: "mcp:docs".into(),
                    source: "mcp:docs".into(),
                    tool_path: "mcp:docs::search".into(),
                    context_usage_kind: "mcp_tools".into(),
                },
                protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::Mcp {
                    server_key: "docs".to_owned(),
                },
                permission: RuntimeToolPermission::ProductWrite,
                effect: RuntimeToolEffect::ProductMutation,
            }
        }

        async fn execute(&self, _invocation: RuntimeToolInvocation) -> AgentToolResult {
            AgentToolResult::Completed {
                output: serde_json::json!({"result": "scoped"}),
            }
        }
    }

    #[tokio::test]
    async fn required_vfs_tool_executes_through_final_broker() {
        let broker =
            PlatformToolBroker::new([Arc::new(MountsList) as Arc<_>], Arc::new(Allow)).unwrap();
        let result = broker
            .invoke(
                resolved_context(),
                AgentToolName::new("mounts_list").unwrap(),
                serde_json::json!({}),
            )
            .await
            .unwrap();
        assert_eq!(
            result,
            AgentToolResult::Completed {
                output: serde_json::json!({"mounts": ["main"]})
            }
        );
    }

    #[tokio::test]
    async fn unknown_tool_is_typed_rejection() {
        let broker =
            PlatformToolBroker::new([Arc::new(MountsList) as Arc<_>], Arc::new(Allow)).unwrap();
        let error = broker
            .invoke(
                resolved_context(),
                AgentToolName::new("missing").unwrap(),
                serde_json::json!({}),
            )
            .await
            .unwrap_err();
        assert_eq!(
            error,
            RuntimeToolBrokerError::UnknownTool("missing".to_owned())
        );
    }

    #[test]
    fn empty_catalog_is_rejected_at_composition_time() {
        let error = PlatformToolBroker::new(std::iter::empty(), Arc::new(Allow))
            .err()
            .expect("empty catalog must be rejected");
        assert_eq!(error, RuntimeToolBrokerError::EmptyCatalog);
    }

    #[test]
    fn catalog_exposes_registered_runtime_tools() {
        let broker =
            PlatformToolBroker::new([Arc::new(MountsList) as Arc<_>], Arc::new(Allow)).unwrap();
        assert_eq!(
            broker
                .definitions()
                .into_iter()
                .map(|definition| definition.name.to_string())
                .collect::<Vec<_>>(),
            vec!["mounts_list"]
        );
    }

    #[tokio::test]
    async fn missing_product_grant_is_rejected_before_execution() {
        let broker =
            PlatformToolBroker::new([Arc::new(MountsList) as Arc<_>], Arc::new(Deny)).unwrap();
        let error = broker
            .invoke(
                resolved_context(),
                AgentToolName::new("mounts_list").unwrap(),
                serde_json::json!({}),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            RuntimeToolBrokerError::AuthorizationDenied { code, .. }
                if code == "missing_product_grant"
        ));
    }

    #[tokio::test]
    async fn dynamic_catalog_is_idempotent_and_isolated_by_runtime_thread() {
        let broker =
            PlatformToolBroker::new([Arc::new(MountsList) as Arc<_>], Arc::new(Allow)).unwrap();
        let thread = RuntimeThreadId::new("thread-test").unwrap();
        let first = broker
            .bind_runtime_catalog(
                thread.clone(),
                [Arc::new(DynamicTool) as Arc<dyn RuntimeToolExecutor>],
            )
            .await
            .unwrap();
        let replay = broker
            .bind_runtime_catalog(
                thread,
                [Arc::new(DynamicTool) as Arc<dyn RuntimeToolExecutor>],
            )
            .await
            .unwrap();
        assert_eq!(first, replay);
        assert_eq!(
            broker
                .invoke(
                    resolved_context(),
                    AgentToolName::new("mcp_docs_search").unwrap(),
                    serde_json::json!({})
                )
                .await
                .unwrap(),
            AgentToolResult::Completed {
                output: serde_json::json!({"result": "scoped"})
            }
        );
        let mut other = resolved_context();
        other.runtime_thread_id = RuntimeThreadId::new("another-thread").unwrap();
        assert!(matches!(
            broker
                .invoke(
                    other,
                    AgentToolName::new("mcp_docs_search").unwrap(),
                    serde_json::json!({})
                )
                .await,
            Err(RuntimeToolBrokerError::UnknownTool(_))
        ));
    }

    fn resolved_context() -> RuntimeToolResolvedContext {
        RuntimeToolResolvedContext {
            runtime_thread_id: RuntimeThreadId::new("thread-test").unwrap(),
            binding_generation: AgentBindingGeneration(1),
            source: AgentSourceCoordinate::new("source-test").unwrap(),
            service_instance_id: AgentServiceInstanceId::new("service-test").unwrap(),
            profile_digest: AgentProfileDigest::new("profile-test").unwrap(),
            bound_surface_revision: AgentSurfaceRevision(1),
            bound_surface_digest: AgentSurfaceDigest::new("bound-test").unwrap(),
            applied_surface_revision: AgentSurfaceRevision(1),
            applied_surface_digest: AgentSurfaceDigest::new("applied-test").unwrap(),
            turn_id: AgentTurnId::new("turn-test").unwrap(),
            item_id: Some(AgentItemId::new("item-test").unwrap()),
            effect_id: AgentEffectIdentity::new("effect-test").unwrap(),
            callback_idempotency_key: "callback-test".to_owned(),
            deadline_at_ms: u64::MAX,
        }
    }

    fn provenance() -> RuntimeToolProvenanceEvidence {
        RuntimeToolProvenanceEvidence {
            source_kind: "test".to_owned(),
            source_id: "surface".to_owned(),
            source_revision: 1,
            projection_revision: 1,
            captured_at_ms: 1,
        }
    }
}
