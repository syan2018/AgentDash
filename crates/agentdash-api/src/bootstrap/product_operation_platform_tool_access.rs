use std::sync::Arc;

use agentdash_agent_runtime::{
    PlatformToolBroker, RuntimeToolDefinition, RuntimeToolResolvedContext,
};
use agentdash_agent_service_api::{
    AgentEffectIdentity, AgentSurfaceRevision, AgentToolName, AgentToolResult, AgentTurnId,
};
use agentdash_application::execution_authority::{
    ExecutionAuthority, ExecutionAuthorityRequest, ExecutionAuthorityResolver,
};
use agentdash_application_operation_gateway::{
    OperationAuthorizationScope, OperationEffect, OperationExecutionError,
    OperationInvocationEnvelope, OperationPrincipal, OperationPrincipalRef, OperationReplayPolicy,
    PlatformToolOperation, PlatformToolOperationAccess, scope_project_id,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub(crate) struct ProductPlatformToolOperationAccess {
    execution_authorities: Arc<dyn ExecutionAuthorityResolver>,
    broker: Arc<PlatformToolBroker>,
}

impl ProductPlatformToolOperationAccess {
    pub(crate) fn new(
        execution_authorities: Arc<dyn ExecutionAuthorityResolver>,
        broker: Arc<PlatformToolBroker>,
    ) -> Self {
        Self {
            execution_authorities,
            broker,
        }
    }

    async fn resolve_agent_surface(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
    ) -> Result<ExecutionAuthority, OperationExecutionError> {
        let OperationPrincipalRef::AgentRunAgent { run_id, agent_id } = principal.principal_ref()
        else {
            return Err(OperationExecutionError::CapabilitiesDenied {
                missing: vec!["platform_tool.agent_run_principal".to_string()],
            });
        };
        let target = AgentRunTarget {
            run_id: *run_id,
            agent_id: *agent_id,
        };
        let binding = self
            .execution_authorities
            .resolve(ExecutionAuthorityRequest::for_target(target))
            .await
            .map_err(|error| OperationExecutionError::NotReady {
                code: error.code().to_string(),
                message: error.to_string(),
            })?;
        if scope_project_id(&scope.scope_ref) != Some(binding.project_id()) {
            return Err(OperationExecutionError::CapabilitiesDenied {
                missing: vec!["agent_run.project_scope".to_string()],
            });
        }
        if !scope.authority_revision.is_empty()
            && scope.authority_revision != binding.revision_token()
        {
            return Err(OperationExecutionError::NotReady {
                code: "stale_execution_authority".to_string(),
                message: "Operation surface authority changed during projection".to_string(),
            });
        }
        Ok(binding)
    }
}

#[async_trait]
impl PlatformToolOperationAccess for ProductPlatformToolOperationAccess {
    async fn discover_tools(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<Vec<PlatformToolOperation>, OperationExecutionError> {
        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        if !matches!(
            principal.principal_ref(),
            OperationPrincipalRef::AgentRunAgent { .. }
        ) {
            return Ok(Vec::new());
        }
        self.resolve_agent_surface(principal, scope).await?;
        Ok(self
            .broker
            .definitions()
            .into_iter()
            .filter_map(platform_tool_operation)
            .collect())
    }

    async fn invoke_tool(
        &self,
        envelope: OperationInvocationEnvelope,
        tool_name: &str,
        cancel: CancellationToken,
    ) -> Result<serde_json::Value, OperationExecutionError> {
        let binding = self
            .resolve_agent_surface(&envelope.principal, &envelope.scope)
            .await?;
        let context = RuntimeToolResolvedContext {
            runtime_thread_id: binding.runtime_thread_id().clone(),
            host_binding_generation: None,
            applied_surface_revision: AgentSurfaceRevision(binding.revision()),
            turn_id: AgentTurnId::new(envelope.trace.trace_id.clone())
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?,
            item_id: None,
            effect_id: AgentEffectIdentity::new(envelope.trace.invocation_id.clone())
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?,
            invocation_id: envelope
                .idempotency_key
                .unwrap_or_else(|| envelope.trace.invocation_id.clone()),
            deadline_at_ms: envelope.deadline.timestamp_millis().max(0) as u64,
        };
        let tool = AgentToolName::new(tool_name)
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        let result = tokio::select! {
            _ = cancel.cancelled() => return Err(OperationExecutionError::Cancelled),
            result = self.broker.invoke(context, tool, envelope.input) => result,
        }
        .map_err(|error| OperationExecutionError::NotReady {
            code: "platform_tool_broker_rejected".to_string(),
            message: error.to_string(),
        })?;
        match result {
            AgentToolResult::Completed { output } => Ok(output),
            AgentToolResult::Rejected { code, message } => {
                Err(OperationExecutionError::NotReady { code, message })
            }
            AgentToolResult::Failed { code, message } => Err(
                OperationExecutionError::provider_failed(format!("{code}: {message}")),
            ),
        }
    }
}

fn platform_tool_operation(definition: RuntimeToolDefinition) -> Option<PlatformToolOperation> {
    let (provider_key, effect, replay_policy) = match definition.name.as_str() {
        "mounts_list" | "fs_read" | "fs_glob" | "fs_grep" => (
            "vfs",
            OperationEffect::Read,
            OperationReplayPolicy::ReplaySafe,
        ),
        "fs_apply_patch" => (
            "vfs",
            OperationEffect::LocalMutation,
            OperationReplayPolicy::NonReplayable,
        ),
        "shell_exec" => (
            "process",
            OperationEffect::LocalMutation,
            OperationReplayPolicy::NonReplayable,
        ),
        "task_read" => (
            "task",
            OperationEffect::Read,
            OperationReplayPolicy::ReplaySafe,
        ),
        "task_write" => (
            "task",
            OperationEffect::LocalMutation,
            OperationReplayPolicy::NonReplayable,
        ),
        _ => return None,
    };
    Some(PlatformToolOperation {
        provider_key: provider_key.to_string(),
        tool_name: definition.name.to_string(),
        description: definition.description,
        input_schema: definition.parameters_schema,
        output_schema: serde_json::json!(true),
        effect,
        replay_policy,
        required_capability: definition.provenance.capability_key,
        provenance: definition.provenance.tool_path,
    })
}

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime::{
        RuntimeToolEffect, RuntimeToolPermission, RuntimeToolProvenance, ToolProtocolProjector,
    };

    use super::*;

    fn definition(name: &str) -> RuntimeToolDefinition {
        RuntimeToolDefinition {
            name: AgentToolName::new(name).expect("tool name"),
            description: name.to_string(),
            parameters_schema: serde_json::json!({"type": "object"}),
            provenance: RuntimeToolProvenance {
                capability_key: "capability".to_string(),
                source: "platform:test".to_string(),
                tool_path: format!("capability::{name}"),
                context_usage_kind: "system_tools".to_string(),
            },
            protocol_projector: ToolProtocolProjector::Dynamic,
            permission: RuntimeToolPermission::ProductRead,
            effect: RuntimeToolEffect::ReadOnly,
        }
    }

    #[test]
    fn exposure_registry_contains_only_declared_native_operations() {
        let exposed = [
            "mounts_list",
            "fs_read",
            "fs_glob",
            "fs_grep",
            "fs_apply_patch",
            "shell_exec",
            "task_read",
            "task_write",
        ];
        assert!(exposed.iter().all(|name| {
            platform_tool_operation(definition(name))
                .is_some_and(|operation| operation.tool_name == *name)
        }));
        for control in [
            "workspace_module_invoke",
            "operation_script_run",
            "complete_lifecycle_node",
            "companion_request",
            "wait",
        ] {
            assert!(platform_tool_operation(definition(control)).is_none());
        }
    }
}
