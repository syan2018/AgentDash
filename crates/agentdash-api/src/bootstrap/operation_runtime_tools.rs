use std::sync::Arc;

use agentdash_application_operation_gateway::{
    OperationGateway, OperationInvocationCommand, OperationPrincipal, OperationTraceContext,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModuleOperationRef,
};
use agentdash_domain::interaction::{
    InteractionDefinitionRepository, InteractionDefinitionRevision, InteractionDefinitionStatus,
    InteractionOwner,
};
use agentdash_domain::operation::{OperationOriginRef, OperationRef, OperationScopeRef};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_spi::{
    AgentTool, AgentToolError, AgentToolResult, ConnectorError, ContentPart, DynAgentTool,
    ToolUpdateCallback,
};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::runtime_gateway::SharedOperationGatewayHandle;
use agentdash_application_ports::operation_script::OperationScriptEngine;
use agentdash_workspace_module::extension_runtime::extension_runtime_projection_from_installations;
use agentdash_workspace_module::workspace_module::{
    build_workspace_module_presentation, build_workspace_modules,
};

#[derive(Clone, Default)]
pub struct SharedOperationScriptEngineHandle {
    inner: Arc<RwLock<Option<Arc<dyn OperationScriptEngine>>>>,
}

impl SharedOperationScriptEngineHandle {
    pub async fn set(&self, engine: Arc<dyn OperationScriptEngine>) {
        *self.inner.write().await = Some(engine);
    }

    async fn get(&self) -> Option<Arc<dyn OperationScriptEngine>> {
        self.inner.read().await.clone()
    }
}

#[derive(Debug, Clone)]
pub struct AgentRunOperationSurfaceTarget {
    pub project_id: uuid::Uuid,
    pub run_id: uuid::Uuid,
    pub agent_id: uuid::Uuid,
    pub frame_id: uuid::Uuid,
    pub workspace_module_enabled: bool,
}

#[derive(Clone)]
pub struct PlatformToolBinding {
    pub tool: DynAgentTool,
    pub capability_key: String,
    pub tool_path: String,
}

#[async_trait]
pub trait AgentRunPlatformToolFactory: Send + Sync {
    async fn build_tools(
        &self,
        target: AgentRunOperationSurfaceTarget,
    ) -> Result<Vec<PlatformToolBinding>, ConnectorError>;
}

#[derive(Debug, Clone)]
pub struct AgentRunExecutionRef {
    pub project_id: uuid::Uuid,
    pub run_id: uuid::Uuid,
    pub agent_id: uuid::Uuid,
    pub frame_id: uuid::Uuid,
}

#[derive(Debug, Clone)]
pub struct WorkspaceModulePresentationRequest {
    pub execution: AgentRunExecutionRef,
    pub definition: InteractionDefinitionRevision,
}

#[derive(Debug, Clone)]
pub struct WorkspaceModulePresentationTarget {
    pub instance_id: uuid::Uuid,
    pub attachment_id: Option<uuid::Uuid>,
}

#[async_trait]
pub trait WorkspaceModulePresentationPort: Send + Sync {
    async fn attach_for_presentation(
        &self,
        request: WorkspaceModulePresentationRequest,
    ) -> Result<WorkspaceModulePresentationTarget, String>;
}

pub struct ApiWorkspaceModulePresentationPort {
    definitions: Arc<dyn InteractionDefinitionRepository>,
    instances: Arc<dyn agentdash_domain::interaction::InteractionInstanceRepository>,
}

impl ApiWorkspaceModulePresentationPort {
    pub fn new(
        definitions: Arc<dyn InteractionDefinitionRepository>,
        instances: Arc<dyn agentdash_domain::interaction::InteractionInstanceRepository>,
    ) -> Self {
        Self {
            definitions,
            instances,
        }
    }
}

#[async_trait]
impl WorkspaceModulePresentationPort for ApiWorkspaceModulePresentationPort {
    async fn attach_for_presentation(
        &self,
        request: WorkspaceModulePresentationRequest,
    ) -> Result<WorkspaceModulePresentationTarget, String> {
        use agentdash_domain::interaction::{
            AttachmentCapabilityProjection, AttachmentSubject, InteractionAttachment,
            InteractionAttachmentRole, InteractionInstance, InteractionInstanceStatus,
            InteractionOwner, InteractionRetention,
        };
        let definition = self
            .definitions
            .get(request.definition.definition_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "InteractionDefinition 不存在".to_string())?;
        if definition.current_revision_id != request.definition.revision_id
            || definition.project_id != request.execution.project_id
        {
            return Err("presentation definition revision 不属于当前 Project surface".to_string());
        }
        let owner = InteractionOwner::Project(request.execution.project_id);
        let mut instance = self
            .instances
            .list_by_owner(&owner)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|instance| {
                instance.status == InteractionInstanceStatus::Open
                    && instance.definition_revision_id == request.definition.revision_id
            });
        if instance.is_none() {
            let created = InteractionInstance::new_v1(
                owner,
                request.definition.definition_id,
                request.definition.revision_id,
                request.definition.initial_state.clone(),
                InteractionRetention { retain_until: None },
            )
            .map_err(|error| error.to_string())?;
            self.instances
                .create(&created)
                .await
                .map_err(|error| error.to_string())?;
            instance = Some(created);
        }
        let instance = instance.expect("instance 已创建或已找到");
        let attachment = InteractionAttachment {
            id: uuid::Uuid::new_v4(),
            instance_id: instance.id,
            subject: AttachmentSubject::AgentRun {
                run_id: request.execution.run_id,
                agent_id: request.execution.agent_id,
            },
            role: InteractionAttachmentRole::Renderer,
            capabilities: AttachmentCapabilityProjection::for_role(
                InteractionAttachmentRole::Renderer,
            ),
            created_at: chrono::Utc::now(),
            detached_at: None,
        };
        attachment.validate().map_err(|error| error.to_string())?;
        let attachment_id = match self.instances.attach(&attachment).await {
            Ok(()) => Some(attachment.id),
            Err(agentdash_domain::interaction::InteractionError::PersistenceConflict {
                ..
            }) => None,
            Err(error) => return Err(error.to_string()),
        };
        Ok(WorkspaceModulePresentationTarget {
            instance_id: instance.id,
            attachment_id,
        })
    }
}

#[derive(Clone)]
pub struct WorkspaceModuleRuntimeToolProvider {
    installations: Arc<dyn ProjectExtensionInstallationRepository>,
    definitions: Arc<dyn InteractionDefinitionRepository>,
    gateway: SharedOperationGatewayHandle,
    script_engine: SharedOperationScriptEngineHandle,
    presentation: Arc<dyn WorkspaceModulePresentationPort>,
}

impl WorkspaceModuleRuntimeToolProvider {
    pub(crate) fn new(
        installations: Arc<dyn ProjectExtensionInstallationRepository>,
        definitions: Arc<dyn InteractionDefinitionRepository>,
        gateway: SharedOperationGatewayHandle,
        script_engine: SharedOperationScriptEngineHandle,
        presentation: Arc<dyn WorkspaceModulePresentationPort>,
    ) -> Self {
        Self {
            installations,
            definitions,
            gateway,
            script_engine,
            presentation,
        }
    }

    async fn project_definitions(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<InteractionDefinitionRevision>, ConnectorError> {
        let definitions = self
            .definitions
            .list_canvas_by_project(project_id)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let mut revisions = Vec::new();
        for definition in definitions {
            if definition.status != InteractionDefinitionStatus::Active
                || !matches!(definition.owner, InteractionOwner::Project(owner) if owner == project_id)
            {
                continue;
            }
            let revision = self
                .definitions
                .get_revision(definition.current_revision_id)
                .await
                .map_err(|error| ConnectorError::Runtime(error.to_string()))?
                .ok_or_else(|| {
                    ConnectorError::Runtime(
                        "InteractionDefinition current revision 缺失".to_string(),
                    )
                })?;
            revisions.push(revision);
        }
        Ok(revisions)
    }
}

#[async_trait]
impl AgentRunPlatformToolFactory for WorkspaceModuleRuntimeToolProvider {
    async fn build_tools(
        &self,
        target: AgentRunOperationSurfaceTarget,
    ) -> Result<Vec<PlatformToolBinding>, ConnectorError> {
        if !target.workspace_module_enabled {
            return Ok(Vec::new());
        }
        let execution = AgentRunExecutionRef {
            project_id: target.project_id,
            run_id: target.run_id,
            agent_id: target.agent_id,
            frame_id: target.frame_id,
        };
        let gateway = self.gateway.get().await.ok_or_else(|| {
            ConnectorError::InvalidConfig("canonical OperationGateway 尚未装配".to_string())
        })?;
        let principal = OperationPrincipal::server_resolved(
            agentdash_domain::operation::OperationPrincipalRef::AgentRunAgent {
                run_id: execution.run_id,
                agent_id: execution.agent_id,
            },
        );
        let scope = OperationScopeRef::Project {
            project_id: execution.project_id,
        };
        let surface = gateway
            .surface_current(
                &principal,
                &scope,
                &OperationOriginRef::AgentTool,
                CancellationToken::new(),
            )
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let operations = surface
            .catalog
            .descriptors()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let authority_revision = surface.authority_revision.clone();
        let granted_capabilities = surface.granted_capabilities.clone();
        let installations = self
            .installations
            .list_enabled_by_project(execution.project_id)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let extensions = extension_runtime_projection_from_installations(installations)
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let definitions = self.project_definitions(execution.project_id).await?;
        let modules = Arc::new(build_workspace_modules(
            &extensions,
            &definitions,
            &operations,
        ));
        let context = ToolContext {
            execution,
            principal,
            scope,
            gateway,
            authority_revision,
            granted_capabilities,
            operations: Arc::new(operations.clone()),
            script_engine: self.script_engine.get().await,
        };
        let tools: Vec<DynAgentTool> = vec![
            Arc::new(ListTool {
                modules: modules.clone(),
            }),
            Arc::new(DescribeTool {
                modules: modules.clone(),
            }),
            Arc::new(InvokeTool {
                context: context.clone(),
                modules: modules.clone(),
            }),
            Arc::new(PresentTool {
                context: context.clone(),
                modules,
                definitions: Arc::new(definitions),
                presentation: self.presentation.clone(),
            }),
            Arc::new(OperationScriptTool {
                context: context.clone(),
            }),
        ];
        Ok(to_platform_bindings(tools))
    }
}

fn to_platform_bindings(tools: Vec<DynAgentTool>) -> Vec<PlatformToolBinding> {
    tools
        .into_iter()
        .map(|tool| PlatformToolBinding {
            tool_path: format!("workspace_module::{}", tool.name()),
            capability_key: "workspace_module".to_string(),
            tool,
        })
        .collect()
}

#[derive(Clone)]
struct ToolContext {
    execution: AgentRunExecutionRef,
    principal: OperationPrincipal,
    scope: OperationScopeRef,
    gateway: Arc<OperationGateway>,
    authority_revision: String,
    granted_capabilities: std::collections::BTreeSet<String>,
    operations: Arc<Vec<agentdash_application_operation_gateway::OperationDescriptor>>,
    script_engine: Option<Arc<dyn OperationScriptEngine>>,
}

fn ok_json(value: serde_json::Value) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(value.to_string())],
        is_error: false,
        details: Some(value),
    }
}

struct ListTool {
    modules: Arc<Vec<WorkspaceModuleDescriptor>>,
}

#[async_trait]
impl AgentTool for ListTool {
    fn name(&self) -> &str {
        "workspace_module_list"
    }
    fn description(&self) -> &str {
        "List actor-visible Workspace Modules from canonical Interaction and Operation surfaces."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","additionalProperties":false})
    }
    async fn execute(
        &self,
        _: &str,
        _: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        Ok(ok_json(
            serde_json::to_value(
                self.modules
                    .iter()
                    .map(|module| &module.summary)
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?,
        ))
    }
}

#[derive(Deserialize)]
struct ModuleArgs {
    module_id: String,
}

struct DescribeTool {
    modules: Arc<Vec<WorkspaceModuleDescriptor>>,
}

#[async_trait]
impl AgentTool for DescribeTool {
    fn name(&self) -> &str {
        "workspace_module_describe"
    }
    fn description(&self) -> &str {
        "Describe one Workspace Module including exact canonical OperationRefs."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"module_id":{"type":"string"}},"required":["module_id"],"additionalProperties":false})
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let args: ModuleArgs = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;
        let module = self
            .modules
            .iter()
            .find(|module| module.summary.module_id == args.module_id)
            .ok_or_else(|| {
                AgentToolError::InvalidArguments(format!(
                    "WorkspaceModule 不存在: {}",
                    args.module_id
                ))
            })?;
        Ok(ok_json(serde_json::to_value(module).map_err(|error| {
            AgentToolError::ExecutionFailed(error.to_string())
        })?))
    }
}

#[derive(Deserialize)]
struct InvokeArgs {
    operation_ref: WorkspaceModuleOperationRef,
    #[serde(default)]
    input: serde_json::Value,
}

struct InvokeTool {
    context: ToolContext,
    modules: Arc<Vec<WorkspaceModuleDescriptor>>,
}

#[async_trait]
impl AgentTool for InvokeTool {
    fn name(&self) -> &str {
        "workspace_module_invoke"
    }
    fn description(&self) -> &str {
        "Invoke an exact OperationRef through the canonical OperationGateway."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"operation_ref":{"type":"object","properties":{"namespace":{"type":"string"},"provider_key":{"type":"string"},"operation_key":{"type":"string"},"contract_version":{"type":"integer","minimum":1}},"required":["namespace","provider_key","operation_key","contract_version"],"additionalProperties":false},"input":{}},"required":["operation_ref"],"additionalProperties":false})
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let args: InvokeArgs = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;
        let exact = self
            .modules
            .iter()
            .flat_map(|module| &module.operations)
            .any(|operation| operation.operation_ref == args.operation_ref);
        if !exact {
            return Err(AgentToolError::InvalidArguments(
                "OperationRef 不在当前 actor surface".to_string(),
            ));
        }
        let operation_ref = OperationRef::new(
            args.operation_ref.namespace,
            args.operation_ref.provider_key,
            args.operation_ref.operation_key,
            args.operation_ref.contract_version,
        )
        .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;
        let result = self
            .context
            .gateway
            .invoke(
                OperationInvocationCommand {
                    operation_ref,
                    input: args.input,
                    principal: self.context.principal.clone(),
                    scope_ref: self.context.scope.clone(),
                    origin: OperationOriginRef::AgentTool,
                    trace: OperationTraceContext::root(),
                    deadline: chrono::Utc::now() + chrono::Duration::seconds(30),
                    idempotency_key: None,
                    attachment_ref: None,
                },
                cancel,
            )
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        Ok(ok_json(serde_json::to_value(result).map_err(|error| {
            AgentToolError::ExecutionFailed(error.to_string())
        })?))
    }
}

#[derive(Deserialize)]
struct PresentArgs {
    module_id: String,
    #[serde(default = "default_view_key")]
    view_key: String,
    #[serde(default)]
    payload: Option<serde_json::Value>,
}
fn default_view_key() -> String {
    "preview".to_string()
}

struct PresentTool {
    context: ToolContext,
    modules: Arc<Vec<WorkspaceModuleDescriptor>>,
    definitions: Arc<Vec<InteractionDefinitionRevision>>,
    presentation: Arc<dyn WorkspaceModulePresentationPort>,
}

#[async_trait]
impl AgentTool for PresentTool {
    fn name(&self) -> &str {
        "workspace_module_present"
    }
    fn description(&self) -> &str {
        "Attach and present a canonical Interaction-backed Workspace Module."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"module_id":{"type":"string"},"view_key":{"type":"string","default":"preview"},"payload":{}},"required":["module_id"],"additionalProperties":false})
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let args: PresentArgs = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;
        let module = self
            .modules
            .iter()
            .find(|module| module.summary.module_id == args.module_id)
            .ok_or_else(|| {
                AgentToolError::InvalidArguments(format!(
                    "WorkspaceModule 不存在: {}",
                    args.module_id
                ))
            })?;
        let mut diagnostics = None;
        let mut interaction_instance_id = None;
        if let Some(definition_id) = args
            .module_id
            .strip_prefix("canvas:")
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
        {
            let definition = self
                .definitions
                .iter()
                .find(|revision| revision.definition_id == definition_id)
                .cloned()
                .ok_or_else(|| {
                    AgentToolError::InvalidArguments(
                        "Canvas definition revision 不存在".to_string(),
                    )
                })?;
            let target = self
                .presentation
                .attach_for_presentation(WorkspaceModulePresentationRequest {
                    execution: self.context.execution.clone(),
                    definition,
                })
                .await
                .map_err(AgentToolError::ExecutionFailed)?;
            interaction_instance_id = Some(target.instance_id);
            diagnostics = Some(serde_json::json!({
                "definition_uri": format!("canvas://{definition_id}"),
                "instance_id": target.instance_id,
                "attachment_id": target.attachment_id
            }));
        }
        let mut result =
            build_workspace_module_presentation(module, &args.view_key, args.payload, diagnostics)
                .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;
        if let Some(instance_id) = interaction_instance_id {
            result.presentation_uri = format!("interaction://{instance_id}");
        }
        Ok(ok_json(serde_json::to_value(result).map_err(|error| {
            AgentToolError::ExecutionFailed(error.to_string())
        })?))
    }
}

#[derive(Deserialize)]
struct OperationScriptArgs {
    source: String,
    #[serde(default)]
    input: serde_json::Value,
}

struct OperationScriptTool {
    context: ToolContext,
}

#[async_trait]
impl AgentTool for OperationScriptTool {
    fn name(&self) -> &str {
        "operation_script"
    }

    fn description(&self) -> &str {
        "Run a bounded async Rhai program that composes exact operations from the current actor surface."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "source": { "type": "string" },
                "input": {}
            },
            "required": ["source"],
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        use agentdash_application_operation_gateway::GatewayOperationScriptExecutor;
        use agentdash_application_ports::operation_script::{
            OPERATION_SCRIPT_HOST_API_V1, OperationScriptAllowedOperation,
            OperationScriptExecutionContext, OperationScriptLimits,
            OperationScriptPreflightRequest, OperationScriptProgram, OperationScriptRunRequest,
            RHAI_V1_DIALECT,
        };
        use sha2::{Digest, Sha256};

        let args: OperationScriptArgs = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;
        let engine = self.context.script_engine.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed("OperationScript engine 尚未装配".to_string())
        })?;
        let allowed_operations = self
            .context
            .operations
            .iter()
            .map(|descriptor| {
                let encoded = serde_json::to_vec(descriptor).unwrap_or_default();
                OperationScriptAllowedOperation {
                    operation_ref: descriptor.operation_ref.clone(),
                    descriptor_digest: format!("sha256:{:x}", Sha256::digest(encoded)),
                    effect: descriptor.effect.clone(),
                    replay_policy: descriptor.replay_policy.clone(),
                    recursive_operation_script: false,
                }
            })
            .collect();
        let script_context = OperationScriptExecutionContext {
            principal: self.context.principal.principal_ref().clone(),
            scope: self.context.scope.clone(),
            authority_revision: self.context.authority_revision.clone(),
            granted_capabilities: self.context.granted_capabilities.clone(),
            origin: OperationOriginRef::AgentTool,
            trace_id: tool_call_id.to_string(),
            attachment_ref: None,
        };
        let program = OperationScriptProgram {
            dialect: RHAI_V1_DIALECT.to_string(),
            host_api_version: OPERATION_SCRIPT_HOST_API_V1,
            source: args.source,
            input: args.input,
            allowed_operations,
            limits: OperationScriptLimits::default(),
        };
        let preflight = engine
            .preflight(
                OperationScriptPreflightRequest {
                    program: program.clone(),
                    context: script_context.clone(),
                },
                cancel.clone(),
            )
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        let outcome = engine
            .run(
                OperationScriptRunRequest {
                    program,
                    context: script_context,
                    token: preflight.token,
                },
                Arc::new(GatewayOperationScriptExecutor::new(
                    self.context.gateway.clone(),
                )),
                cancel,
            )
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        Ok(ok_json(serde_json::to_value(outcome).map_err(|error| {
            AgentToolError::ExecutionFailed(error.to_string())
        })?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubTool;

    #[async_trait]
    impl AgentTool for StubTool {
        fn name(&self) -> &str {
            "workspace_module_list"
        }
        fn description(&self) -> &str {
            "stub"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        async fn execute(
            &self,
            _: &str,
            _: serde_json::Value,
            _: CancellationToken,
            _: Option<ToolUpdateCallback>,
        ) -> Result<AgentToolResult, AgentToolError> {
            Ok(ok_json(serde_json::json!([])))
        }
    }

    #[test]
    fn platform_bindings_keep_workspace_capability_provenance() {
        let bindings = to_platform_bindings(vec![Arc::new(StubTool)]);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].capability_key, "workspace_module");
        assert_eq!(
            bindings[0].tool_path,
            "workspace_module::workspace_module_list"
        );
        assert_eq!(bindings[0].tool.name(), "workspace_module_list");
    }
}
