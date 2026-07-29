use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_application_operation_gateway::{
    DynamicOperationProvider, OperationActorKind, OperationAuthorizationScope, OperationDescriptor,
    OperationDispatch, OperationExecutionError, OperationExecutionPolicy,
    OperationInvocationEnvelope, OperationOriginRef, OperationPlacement, OperationPrincipal,
    OperationProvenance, OperationReadiness, scope_project_id,
};
use agentdash_application_ports::product_runtime_tool::ProductRuntimeToolOutcome;
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::interaction::{
    InteractionDefinitionRepository, InteractionDefinitionRevision, InteractionDefinitionStatus,
    InteractionOwner,
};
use agentdash_domain::operation::{
    OperationEffect, OperationPrincipalRef, OperationProviderRef, OperationRef,
    OperationReplayPolicy,
};
use agentdash_domain::workflow::LifecycleAgentRepository;
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::mount_surface::CanvasMountMaterializationPort;
use super::operation_strategy::{
    CanvasOperationContext, CanvasOperationStrategies, CanvasOperationStrategyDeps,
};

pub const CANVAS_OPERATION_NAMESPACE: &str = "canvas";

pub struct CanvasAuthoringOperationProvider {
    definitions: Arc<dyn InteractionDefinitionRepository>,
    lifecycle_agents: Arc<dyn LifecycleAgentRepository>,
    strategies: CanvasOperationStrategies,
}

impl CanvasAuthoringOperationProvider {
    pub fn new(
        definitions: Arc<dyn InteractionDefinitionRepository>,
        lifecycle_agents: Arc<dyn LifecycleAgentRepository>,
        mounts: Arc<dyn CanvasMountMaterializationPort>,
    ) -> Self {
        Self {
            definitions: definitions.clone(),
            lifecycle_agents,
            strategies: CanvasOperationStrategies::new(CanvasOperationStrategyDeps {
                definitions,
                mounts,
            }),
        }
    }

    async fn resolve_actor(
        &self,
        principal: &OperationPrincipal,
        project_id: Uuid,
    ) -> Result<(String, Option<AgentRunTarget>), OperationExecutionError> {
        match principal.principal_ref() {
            OperationPrincipalRef::User { user_id } => Ok((user_id.clone(), None)),
            OperationPrincipalRef::AgentRunAgent { run_id, agent_id } => {
                let agent = self
                    .lifecycle_agents
                    .get(*agent_id)
                    .await
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "canvas_agent_not_found".to_owned(),
                        message: format!("Lifecycle Agent 不存在: {agent_id}"),
                    })?;
                if agent.run_id != *run_id || agent.project_id != project_id {
                    return Err(OperationExecutionError::CapabilitiesDenied {
                        missing: vec!["canvas.agent_project_scope".to_owned()],
                    });
                }
                Ok((
                    agent.created_by_user_id,
                    Some(AgentRunTarget {
                        run_id: *run_id,
                        agent_id: *agent_id,
                    }),
                ))
            }
            OperationPrincipalRef::WorkflowNode { .. }
            | OperationPrincipalRef::ExtensionInstallation { .. } => {
                Err(OperationExecutionError::CapabilitiesDenied {
                    missing: vec!["canvas.authoring_actor".to_owned()],
                })
            }
        }
    }

    async fn visible_definitions(
        &self,
        project_id: Uuid,
        user_id: &str,
    ) -> Result<Vec<InteractionDefinitionRevision>, OperationExecutionError> {
        let definitions = self
            .definitions
            .list_canvas_by_project(project_id)
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
        let mut revisions = Vec::new();
        for definition in definitions {
            if definition.status != InteractionDefinitionStatus::Active
                || !match &definition.owner {
                    InteractionOwner::Project(owner) => *owner == project_id,
                    InteractionOwner::User(owner) => owner == user_id,
                }
            {
                continue;
            }
            revisions.push(
                self.definitions
                    .get_revision(definition.current_revision_id)
                    .await
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "canvas_revision_missing".to_owned(),
                        message: format!(
                            "Canvas current revision 不存在: {}",
                            definition.current_revision_id
                        ),
                    })?,
            );
        }
        Ok(revisions)
    }
}

#[async_trait]
impl DynamicOperationProvider for CanvasAuthoringOperationProvider {
    fn surface_source(&self) -> &'static str {
        "canvas_authoring"
    }

    fn owns_provider(&self, provider: &OperationProviderRef) -> bool {
        provider.namespace == CANVAS_OPERATION_NAMESPACE
    }

    async fn discover(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        _origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<Vec<OperationDescriptor>, OperationExecutionError> {
        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        let Some(project_id) = scope_project_id(&scope.scope_ref) else {
            return Ok(Vec::new());
        };
        self.resolve_actor(principal, project_id).await?;
        ["create", "attach", "copy"]
            .into_iter()
            .map(|operation| descriptor(project_id, operation))
            .collect()
    }

    async fn resolve_placement(
        &self,
        _descriptor: &OperationDescriptor,
        _principal: &OperationPrincipal,
        _scope: &OperationAuthorizationScope,
        _origin: &OperationOriginRef,
        _cancel: CancellationToken,
    ) -> Result<OperationPlacement, OperationExecutionError> {
        Ok(OperationPlacement::Cloud)
    }

    async fn invoke(
        &self,
        descriptor: &OperationDescriptor,
        envelope: OperationInvocationEnvelope,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError> {
        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        let project_id =
            Uuid::parse_str(&descriptor.operation_ref.provider.provider_key).map_err(|_| {
                OperationExecutionError::invalid_request(
                    "Canvas authoring provider key 必须是 Project UUID",
                )
            })?;
        if scope_project_id(&envelope.scope.scope_ref) != Some(project_id) {
            return Err(OperationExecutionError::CapabilitiesDenied {
                missing: vec!["canvas.project_scope".to_owned()],
            });
        }
        let (user_id, agent_target) = self.resolve_actor(&envelope.principal, project_id).await?;
        let visible_definitions = self.visible_definitions(project_id, &user_id).await?;
        let idempotency_key = envelope
            .idempotency_key
            .as_deref()
            .unwrap_or(&envelope.trace.invocation_id);
        let operation = format!("canvas.{}", descriptor.operation_ref.operation_key);
        let result = self
            .strategies
            .execute(
                &operation,
                CanvasOperationContext {
                    project_id,
                    user_id: &user_id,
                    agent_target: agent_target.as_ref(),
                    idempotency_key,
                    visible_definitions: &visible_definitions,
                },
                envelope.input,
            )
            .await
            .map_err(map_strategy_error)?;
        Ok(json!({
            "action": result.action,
            "canvas_id": result.revision.definition_id,
            "canvas_mount_id": result.revision.authoring_mount_id,
            "definition_revision_id": result.revision.revision_id,
            "title": result.revision.title,
            "description": result.revision.description,
            "entry_file": result.revision.source_bundle.entry_file,
            "source_bundle_digest": result.revision.source_bundle.digest,
        }))
    }
}

pub fn canvas_authoring_operation_ref(
    project_id: Uuid,
    operation: &str,
) -> Result<OperationRef, OperationExecutionError> {
    OperationRef::new(
        CANVAS_OPERATION_NAMESPACE,
        project_id.to_string(),
        operation,
        1,
    )
    .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))
}

fn descriptor(
    project_id: Uuid,
    operation: &'static str,
) -> Result<OperationDescriptor, OperationExecutionError> {
    let operation_ref = canvas_authoring_operation_ref(project_id, operation)?;
    let (description, input_schema, effect, replay_policy) = match operation {
        "create" => (
            "Create a personal Canvas definition and materialize its authoring mount when invoked by an Agent.",
            json!({
                "type":"object",
                "properties":{
                    "canvas_mount_id":{
                        "type":"string",
                        "description":"Optional stable authoring mount id. Empty or invalid mount ids are rejected by the Canvas service."
                    },
                    "title":{
                        "type":"string",
                        "description":"Non-empty Canvas title. Whitespace-only values are rejected by the Canvas service."
                    },
                    "description":{"type":"string"}
                },
                "required":["title"],
                "additionalProperties":false
            }),
            OperationEffect::LocalMutation,
            OperationReplayPolicy::Idempotent,
        ),
        "attach" => (
            "Attach an existing authorized Canvas authoring mount to the current Agent surface.",
            json!({
                "type":"object",
                "properties":{
                    "canvas_mount_id":{
                        "type":"string",
                        "description":"Existing authorized Canvas authoring mount id."
                    }
                },
                "required":["canvas_mount_id"],
                "additionalProperties":false
            }),
            OperationEffect::LocalMutation,
            OperationReplayPolicy::Idempotent,
        ),
        "copy" => (
            "Copy an authorized Canvas to a personal definition and materialize its authoring mount when invoked by an Agent.",
            json!({
                "type":"object",
                "properties":{
                    "source_mount_id":{
                        "type":"string",
                        "description":"Existing authorized Canvas authoring mount id to copy."
                    },
                    "canvas_mount_id":{
                        "type":"string",
                        "description":"Optional stable authoring mount id for the new personal copy."
                    },
                    "title":{"type":"string"},
                    "description":{"type":"string"}
                },
                "required":["source_mount_id"],
                "additionalProperties":false
            }),
            OperationEffect::LocalMutation,
            OperationReplayPolicy::Idempotent,
        ),
        _ => {
            return Err(OperationExecutionError::invalid_request(
                "unsupported Canvas authoring operation",
            ));
        }
    };
    Ok(OperationDescriptor {
        title: format!("Canvas {operation}"),
        description: Some(description.to_owned()),
        input_schema,
        output_schema: json!({"type":"object"}),
        effect,
        replay_policy,
        required_capabilities: BTreeSet::from(["operation.invoke".to_owned()]),
        actor_visibility: BTreeSet::from([OperationActorKind::User, OperationActorKind::Agent]),
        execution_policy: OperationExecutionPolicy::default(),
        readiness: OperationReadiness::Ready,
        provenance: OperationProvenance {
            source: "canvas_builtin_provider".to_owned(),
            artifact_digest: None,
        },
        dispatch: OperationDispatch {
            provider: operation_ref.provider.clone(),
            route: operation_ref.operation_key.clone(),
        },
        operation_ref,
    })
}

#[cfg(test)]
mod tests {
    use agentdash_application_operation_gateway::OperationCatalog;

    use super::*;

    #[test]
    fn canvas_authoring_descriptors_form_a_valid_gateway_catalog() {
        let project_id = Uuid::new_v4();
        let descriptors = ["create", "attach", "copy"]
            .into_iter()
            .map(|operation| descriptor(project_id, operation))
            .collect::<Result<Vec<_>, _>>()
            .expect("Canvas authoring descriptors");

        OperationCatalog::try_new(descriptors)
            .expect("Canvas authoring descriptors must use the Gateway schema subset");
    }
}

fn map_strategy_error(error: ProductRuntimeToolOutcome) -> OperationExecutionError {
    match error {
        ProductRuntimeToolOutcome::Rejected { code, message } => {
            OperationExecutionError::NotReady { code, message }
        }
        ProductRuntimeToolOutcome::Failed { message, .. } => {
            OperationExecutionError::provider_failed(message)
        }
        ProductRuntimeToolOutcome::Completed { .. } => {
            OperationExecutionError::provider_failed("Canvas strategy returned invalid outcome")
        }
    }
}
