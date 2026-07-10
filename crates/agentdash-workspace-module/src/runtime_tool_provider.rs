use std::sync::Arc;

use agentdash_application_runtime_gateway::{
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
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{
    AgentRunExecutionRef, AgentTool, AgentToolError, AgentToolResult, ConnectorError, ContentPart,
    DynAgentTool, ExecutionContext, ToolCluster, ToolUpdateCallback,
};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::extension_runtime::extension_runtime_projection_from_installations;
use crate::workspace_module::{build_workspace_module_presentation, build_workspace_modules};

#[derive(Clone, Default)]
pub struct SharedOperationGatewayHandle {
    inner: Arc<RwLock<Option<Arc<OperationGateway>>>>,
}

impl SharedOperationGatewayHandle {
    pub async fn set(&self, gateway: Arc<OperationGateway>) {
        *self.inner.write().await = Some(gateway);
    }

    pub async fn get(&self) -> Option<Arc<OperationGateway>> {
        self.inner.read().await.clone()
    }
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

#[derive(Clone)]
pub struct WorkspaceModuleRuntimeToolProvider {
    installations: Arc<dyn ProjectExtensionInstallationRepository>,
    definitions: Arc<dyn InteractionDefinitionRepository>,
    gateway: SharedOperationGatewayHandle,
    presentation: Arc<dyn WorkspaceModulePresentationPort>,
}

impl WorkspaceModuleRuntimeToolProvider {
    pub fn new(
        installations: Arc<dyn ProjectExtensionInstallationRepository>,
        definitions: Arc<dyn InteractionDefinitionRepository>,
        gateway: SharedOperationGatewayHandle,
        presentation: Arc<dyn WorkspaceModulePresentationPort>,
    ) -> Self {
        Self {
            installations,
            definitions,
            gateway,
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
impl RuntimeToolProvider for WorkspaceModuleRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        if !context
            .turn
            .capability_state
            .tool
            .enabled_clusters
            .contains(&ToolCluster::WorkspaceModule)
        {
            return Ok(Vec::new());
        }
        let execution = context.session.agent_run_execution.clone().ok_or_else(|| {
            ConnectorError::InvalidConfig(
                "WorkspaceModule tools 缺少 server-owned AgentRunExecutionRef".to_string(),
            )
        })?;
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
        };
        Ok(vec![
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
                context,
                modules,
                definitions: Arc::new(definitions),
                presentation: self.presentation.clone(),
            }),
        ])
    }
}

#[derive(Clone)]
struct ToolContext {
    execution: AgentRunExecutionRef,
    principal: OperationPrincipal,
    scope: OperationScopeRef,
    gateway: Arc<OperationGateway>,
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
