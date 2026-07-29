use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_application_operation_gateway::{
    DynamicOperationProvider, OperationActorKind, OperationAuthorizationScope, OperationDescriptor,
    OperationDispatch, OperationExecutionError, OperationExecutionPolicy,
    OperationInvocationEnvelope, OperationOriginRef, OperationPlacement, OperationPrincipal,
    OperationProvenance, OperationReadiness, scope_project_id,
};
use agentdash_domain::interaction::{
    AttachmentSubject, InteractionDefinitionRepository, InteractionInstance,
    InteractionInstanceRepository, InteractionOwner, InteractionPresentationRepository,
    InteractionRuntimeBinding, ResourceSlotKind, RuntimeBindingAuthorizationRef,
    RuntimeBindingTarget,
};
use agentdash_domain::operation::{
    OperationEffect, OperationPrincipalRef, OperationProviderRef, OperationRef,
    OperationReplayPolicy,
};
use agentdash_domain::workflow::LifecycleAgentRepository;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const CANVAS_RUNTIME_OPERATION_NAMESPACE: &str = "canvas_runtime";
const PRESENTATION_KEY_RENDERER_OBSERVATION: &str = "canvas.renderer-observation";

pub struct CanvasRuntimeOperationProvider {
    definitions: Arc<dyn InteractionDefinitionRepository>,
    instances: Arc<dyn InteractionInstanceRepository>,
    presentations: Arc<dyn InteractionPresentationRepository>,
    lifecycle_agents: Arc<dyn LifecycleAgentRepository>,
}

impl CanvasRuntimeOperationProvider {
    pub fn new(
        definitions: Arc<dyn InteractionDefinitionRepository>,
        instances: Arc<dyn InteractionInstanceRepository>,
        presentations: Arc<dyn InteractionPresentationRepository>,
        lifecycle_agents: Arc<dyn LifecycleAgentRepository>,
    ) -> Self {
        Self {
            definitions,
            instances,
            presentations,
            lifecycle_agents,
        }
    }

    async fn actor(
        &self,
        principal: &OperationPrincipal,
    ) -> Result<CanvasRuntimeActor, OperationExecutionError> {
        match principal.principal_ref() {
            OperationPrincipalRef::User { user_id } => Ok(CanvasRuntimeActor {
                user_id: user_id.clone(),
                agent_subject: None,
            }),
            OperationPrincipalRef::AgentRunAgent { run_id, agent_id } => {
                let agent = self
                    .lifecycle_agents
                    .get(*agent_id)
                    .await
                    .map_err(provider_failed)?
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "canvas_runtime_agent_not_found".into(),
                        message: format!("Lifecycle Agent 不存在: {agent_id}"),
                    })?;
                if agent.run_id != *run_id {
                    return Err(OperationExecutionError::CapabilitiesDenied {
                        missing: vec!["canvas.runtime.agent_target".into()],
                    });
                }
                Ok(CanvasRuntimeActor {
                    user_id: agent.created_by_user_id,
                    agent_subject: Some(AttachmentSubject::AgentRun {
                        run_id: *run_id,
                        agent_id: *agent_id,
                    }),
                })
            }
            _ => Err(OperationExecutionError::CapabilitiesDenied {
                missing: vec!["canvas.runtime.actor".into()],
            }),
        }
    }

    async fn visible_instances(
        &self,
        project_id: Uuid,
        actor: &CanvasRuntimeActor,
    ) -> Result<Vec<InteractionInstance>, OperationExecutionError> {
        let mut instances = self
            .instances
            .list_by_owner(&InteractionOwner::Project(project_id))
            .await
            .map_err(provider_failed)?;
        instances.extend(
            self.instances
                .list_by_owner(&InteractionOwner::User(actor.user_id.clone()))
                .await
                .map_err(provider_failed)?,
        );
        let mut visible = Vec::new();
        for instance in instances {
            let revision = self
                .definitions
                .get_revision(instance.definition_revision_id)
                .await
                .map_err(provider_failed)?;
            if revision
                .as_ref()
                .is_none_or(|revision| revision.project_id != project_id)
            {
                continue;
            }
            if let Some(subject) = &actor.agent_subject {
                let attached = self
                    .instances
                    .list_attachments(instance.id)
                    .await
                    .map_err(provider_failed)?
                    .into_iter()
                    .any(|attachment| &attachment.subject == subject);
                if !attached {
                    continue;
                }
            }
            visible.push(instance);
        }
        Ok(visible)
    }

    async fn authorized_instance(
        &self,
        instance_id: Uuid,
        project_id: Uuid,
        actor: &CanvasRuntimeActor,
    ) -> Result<InteractionInstance, OperationExecutionError> {
        self.visible_instances(project_id, actor)
            .await?
            .into_iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| OperationExecutionError::CapabilitiesDenied {
                missing: vec!["canvas.runtime.instance".into()],
            })
    }
}

#[async_trait]
impl DynamicOperationProvider for CanvasRuntimeOperationProvider {
    fn surface_source(&self) -> &'static str {
        "canvas_runtime"
    }

    fn owns_provider(&self, provider: &OperationProviderRef) -> bool {
        provider.namespace == CANVAS_RUNTIME_OPERATION_NAMESPACE
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
        let actor = self.actor(principal).await?;
        let mut descriptors = Vec::new();
        for instance in self.visible_instances(project_id, &actor).await? {
            for operation in [
                CanvasRuntimeOperation::BindData,
                CanvasRuntimeOperation::Inspect,
                CanvasRuntimeOperation::GetInteractionState,
            ] {
                descriptors.push(runtime_descriptor(instance.id, operation)?);
            }
        }
        Ok(descriptors)
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
        let project_id = scope_project_id(&envelope.scope.scope_ref).ok_or_else(|| {
            OperationExecutionError::invalid_request("Canvas runtime Operation 需要 Project scope")
        })?;
        let instance_id = Uuid::parse_str(&descriptor.operation_ref.provider.provider_key)
            .map_err(|_| {
                OperationExecutionError::invalid_request(
                    "Canvas runtime provider key 必须是 Interaction instance UUID",
                )
            })?;
        let actor = self.actor(&envelope.principal).await?;
        let instance = self
            .authorized_instance(instance_id, project_id, &actor)
            .await?;
        match CanvasRuntimeOperation::parse(&descriptor.operation_ref.operation_key)? {
            CanvasRuntimeOperation::GetInteractionState => {
                let revision = self
                    .definitions
                    .get_revision(instance.definition_revision_id)
                    .await
                    .map_err(provider_failed)?
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "canvas_runtime_revision_missing".into(),
                        message: "Interaction pinned revision 不存在".into(),
                    })?;
                let projection = revision
                    .agent_projection
                    .project(&instance.state)
                    .map_err(provider_failed)?;
                Ok(json!({
                    "instance_id": instance.id,
                    "definition_id": instance.definition_id,
                    "definition_revision_id": instance.definition_revision_id,
                    "state_revision": instance.state_revision,
                    "state": projection,
                }))
            }
            CanvasRuntimeOperation::Inspect => {
                let observation = self
                    .presentations
                    .get_presentation_state(
                        instance.id,
                        &actor.user_id,
                        PRESENTATION_KEY_RENDERER_OBSERVATION,
                    )
                    .await
                    .map_err(provider_failed)?
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "canvas_renderer_observation_missing".into(),
                        message: "Canvas renderer 尚未上报 observation".into(),
                    })?;
                Ok(json!({
                    "instance_id": instance.id,
                    "revision": observation.revision,
                    "observation": observation.value,
                    "observed_at": observation.updated_at,
                }))
            }
            CanvasRuntimeOperation::BindData => {
                let input: BindDataInput = serde_json::from_value(envelope.input)
                    .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
                let revision = self
                    .definitions
                    .get_revision(instance.definition_revision_id)
                    .await
                    .map_err(provider_failed)?
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "canvas_runtime_revision_missing".into(),
                        message: "Interaction pinned revision 不存在".into(),
                    })?;
                let slot = revision
                    .resource_slots
                    .iter()
                    .find(|slot| slot.slot_key == input.slot_key)
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "canvas_resource_slot_missing".into(),
                        message: format!("ResourceSlot 未声明: {}", input.slot_key),
                    })?;
                if slot.kind != ResourceSlotKind::Resource {
                    return Err(OperationExecutionError::invalid_request(
                        "canvas.bind_data 只能绑定 resource slot",
                    ));
                }
                let attachment_id = if let Some(subject) = actor.agent_subject {
                    self.instances
                        .list_attachments(instance.id)
                        .await
                        .map_err(provider_failed)?
                        .into_iter()
                        .find(|attachment| attachment.subject == subject)
                        .map(|attachment| attachment.id)
                } else {
                    None
                };
                let binding = InteractionRuntimeBinding {
                    id: Uuid::new_v4(),
                    instance_id: instance.id,
                    attachment_id,
                    slot_key: input.slot_key,
                    target: RuntimeBindingTarget::Resource {
                        resource_ref: input.source_uri,
                        version_ref: input.version_ref.unwrap_or_else(|| "current".into()),
                    },
                    authorization: RuntimeBindingAuthorizationRef {
                        grant_ref: format!(
                            "operation:{}",
                            descriptor.operation_ref.provider.provider_key
                        ),
                        revision: 1,
                    },
                    created_at: chrono::Utc::now(),
                };
                binding.validate().map_err(provider_failed)?;
                self.instances
                    .upsert_runtime_binding(&binding)
                    .await
                    .map_err(provider_failed)?;
                Ok(json!({
                    "binding_id": binding.id,
                    "instance_id": binding.instance_id,
                    "attachment_id": binding.attachment_id,
                    "slot_key": binding.slot_key,
                    "source_uri": match binding.target {
                        RuntimeBindingTarget::Resource { resource_ref, .. } => resource_ref,
                        _ => unreachable!(),
                    },
                }))
            }
        }
    }
}

struct CanvasRuntimeActor {
    user_id: String,
    agent_subject: Option<AttachmentSubject>,
}

#[derive(Debug, Clone, Copy)]
enum CanvasRuntimeOperation {
    BindData,
    Inspect,
    GetInteractionState,
}

impl CanvasRuntimeOperation {
    fn key(self) -> &'static str {
        match self {
            Self::BindData => "canvas.bind_data",
            Self::Inspect => "canvas.inspect",
            Self::GetInteractionState => "canvas.get_interaction_state",
        }
    }

    fn parse(value: &str) -> Result<Self, OperationExecutionError> {
        match value {
            "canvas.bind_data" => Ok(Self::BindData),
            "canvas.inspect" => Ok(Self::Inspect),
            "canvas.get_interaction_state" => Ok(Self::GetInteractionState),
            _ => Err(OperationExecutionError::invalid_request(
                "未知 Canvas runtime Operation",
            )),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindDataInput {
    slot_key: String,
    source_uri: String,
    #[serde(default)]
    version_ref: Option<String>,
}

fn runtime_descriptor(
    instance_id: Uuid,
    operation: CanvasRuntimeOperation,
) -> Result<OperationDescriptor, OperationExecutionError> {
    let operation_ref = OperationRef::new(
        CANVAS_RUNTIME_OPERATION_NAMESPACE,
        instance_id.to_string(),
        operation.key(),
        1,
    )
    .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
    let (input_schema, effect, replay_policy) = match operation {
        CanvasRuntimeOperation::BindData => (
            json!({
                "type":"object",
                "properties":{
                    "slot_key":{"type":"string","minLength":1},
                    "source_uri":{"type":"string","minLength":1},
                    "version_ref":{"type":"string","minLength":1}
                },
                "required":["slot_key","source_uri"],
                "additionalProperties":false
            }),
            OperationEffect::LocalMutation,
            OperationReplayPolicy::Idempotent,
        ),
        CanvasRuntimeOperation::Inspect | CanvasRuntimeOperation::GetInteractionState => (
            json!({"type":"object","properties":{},"additionalProperties":false}),
            OperationEffect::Read,
            OperationReplayPolicy::ReplaySafe,
        ),
    };
    Ok(OperationDescriptor {
        title: operation.key().into(),
        description: Some(match operation {
            CanvasRuntimeOperation::BindData => {
                "Bind a declared ResourceSlot without mutating Canvas source or Interaction state."
            }
            CanvasRuntimeOperation::Inspect => {
                "Read the latest durable Canvas renderer observation."
            }
            CanvasRuntimeOperation::GetInteractionState => {
                "Read the Agent-allowlisted canonical Interaction state projection."
            }
        }.into()),
        input_schema,
        output_schema: json!({"type":"object"}),
        effect,
        replay_policy,
        required_capabilities: BTreeSet::from(["operation.invoke".into()]),
        actor_visibility: BTreeSet::from([
            OperationActorKind::User,
            OperationActorKind::Agent,
        ]),
        execution_policy: OperationExecutionPolicy::default(),
        readiness: OperationReadiness::Ready,
        provenance: OperationProvenance {
            source: "canvas_builtin_runtime_provider".into(),
            artifact_digest: None,
        },
        dispatch: OperationDispatch {
            provider: operation_ref.provider.clone(),
            route: operation_ref.operation_key.clone(),
        },
        operation_ref,
    })
}

fn provider_failed(error: impl std::fmt::Display) -> OperationExecutionError {
    OperationExecutionError::provider_failed(error.to_string())
}
