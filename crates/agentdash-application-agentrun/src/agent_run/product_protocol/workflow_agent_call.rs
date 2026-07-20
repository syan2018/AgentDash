use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeContextAuthority, ManagedRuntimeContextProvenance,
    ManagedRuntimeInitialContextContribution, ManagedRuntimeInitialContextContributionContent,
    ManagedRuntimeInitialContextMode, ManagedRuntimeInitialContextPackage,
    RuntimeContextContributionId, RuntimeContextPackageId, RuntimeContextSourceRef,
    RuntimeContextSourceRevision, RuntimePayloadDigest, RuntimeThreadId,
};
use agentdash_application_workflow::{
    WorkflowAgentCallContentBlock, WorkflowAgentCallDispatchError,
    WorkflowAgentCallDispatchOutcome, WorkflowAgentCallDispatchPort, WorkflowAgentCallRequest,
    WorkflowAgentCallTargetIntent,
};
use agentdash_domain::agent_run_mailbox::{MailboxMessageOrigin, MailboxSourceIdentity};
use agentdash_domain::workflow::WorkflowAgentCallSourceBindingRef;
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::agent_run::{
    AgentRunProductInputDeliveryPort, AgentRunProductLaunchRequest, AgentRunProductLaunchService,
    AgentRunProductRuntimeProvisioningRequest, DeliverAgentRunProductInput,
};

/// Product-side facts needed to create a Workflow Agent target.
///
/// The Workflow aggregate already persists the request and target. This port only materializes the
/// owner-local LifecycleAgent/AgentFrame documents and resolves the Product launch intent; it does
/// not maintain a second dispatch saga or effect ledger.
#[async_trait]
pub trait WorkflowAgentCallProductPort: Send + Sync {
    async fn materialize_target(
        &self,
        request: &WorkflowAgentCallRequest,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<(), String>;

    async fn resolve_provisioning(
        &self,
        request: &WorkflowAgentCallRequest,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<AgentRunProductRuntimeProvisioningRequest, String>;
}

/// Synchronous Workflow -> AgentRun handoff.
///
/// Recovery relies on deterministic Product coordinates plus the concrete Agent's stable
/// Create/Command effect identities. There is intentionally no Product dispatch saga: a retry
/// rematerializes the same target, inspects/replays the same Agent effect, and submits the same
/// client command identity.
#[derive(Clone)]
pub struct ProductWorkflowAgentCallDispatchService {
    product: Arc<dyn WorkflowAgentCallProductPort>,
    product_launch: Arc<AgentRunProductLaunchService>,
    product_input_delivery: Arc<dyn AgentRunProductInputDeliveryPort>,
}

impl ProductWorkflowAgentCallDispatchService {
    pub fn new(
        product: Arc<dyn WorkflowAgentCallProductPort>,
        product_launch: Arc<AgentRunProductLaunchService>,
        product_input_delivery: Arc<dyn AgentRunProductInputDeliveryPort>,
    ) -> Self {
        Self {
            product,
            product_launch,
            product_input_delivery,
        }
    }

    async fn dispatch_inner(
        &self,
        request: WorkflowAgentCallRequest,
    ) -> Result<WorkflowAgentCallDispatchOutcome, WorkflowAgentCallDispatchError> {
        if !request.validate_payload_digest() {
            return Err(permanent(
                "agent_call_payload_digest_invalid",
                "Workflow AgentCall payload digest 无效",
            ));
        }

        let runtime_thread_id = runtime_thread_id(&request)?;
        let association = match &request.target_intent {
            WorkflowAgentCallTargetIntent::CreateNew { .. } => {
                self.product
                    .materialize_target(&request, &runtime_thread_id)
                    .await
                    .map_err(product_unavailable)?;
                let provisioning = self
                    .product
                    .resolve_provisioning(&request, &runtime_thread_id)
                    .await
                    .map_err(product_unavailable)?;
                if provisioning.target != *request.target_intent.target()
                    || provisioning.runtime_thread_id != runtime_thread_id
                {
                    return Err(permanent(
                        "agent_call_product_authority_mismatch",
                        "Workflow AgentCall Product provisioning 与请求坐标不一致",
                    ));
                }
                let launched = self
                    .product_launch
                    .launch(AgentRunProductLaunchRequest {
                        provisioning,
                        initial_context: Some(initial_context(&request)?),
                        initial_input: Vec::new(),
                    })
                    .await
                    .map_err(|error| {
                        retryable(
                            "agent_call_agent_create_unavailable",
                            format!("Workflow AgentCall 创建 Agent source 失败: {error}"),
                        )
                    })?;
                launched.binding.agent
            }
            WorkflowAgentCallTargetIntent::ContinueCurrent { source_binding, .. } => {
                let binding = self
                    .product_launch
                    .load_product_binding(request.target_intent.target())
                    .await
                    .map_err(|error| {
                        retryable(
                            "agent_call_product_binding_unavailable",
                            format!("Workflow AgentCall 读取 Product binding 失败: {error}"),
                        )
                    })?;
                if binding.runtime_thread_id != runtime_thread_id
                    || binding.agent.service_instance_id.as_str()
                        != source_binding.service_instance_id
                    || binding.agent.source.as_str() != source_binding.source_ref
                {
                    return Err(permanent(
                        "agent_call_product_binding_mismatch",
                        "Workflow AgentCall current target 与 Product authoritative binding 不一致",
                    ));
                }
                binding.agent
            }
        };

        self.product_input_delivery
            .deliver(DeliverAgentRunProductInput {
                target: request.target_intent.target().clone(),
                content: workflow_input_blocks(&request.input),
                source: MailboxSourceIdentity::workflow_orchestrator()
                    .with_source_ref(request.identity.node_path.clone())
                    .with_correlation_ref(request.identity.request_id.clone()),
                origin: MailboxMessageOrigin::System,
                client_command_id: request.identity.request_id.clone(),
            })
            .await
            .map_err(|error| {
                retryable(
                    "agent_call_input_handoff_unavailable",
                    format!("Workflow AgentCall 同步输入交接失败: {error}"),
                )
            })?;
        Ok(WorkflowAgentCallDispatchOutcome::Accepted {
            target: request.target_intent.target().clone(),
            runtime_thread_id: runtime_thread_id.to_string(),
            source_binding: WorkflowAgentCallSourceBindingRef {
                service_instance_id: association.service_instance_id.to_string(),
                source_ref: association.source.to_string(),
            },
        })
    }
}

#[async_trait]
impl WorkflowAgentCallDispatchPort for ProductWorkflowAgentCallDispatchService {
    async fn dispatch(
        &self,
        request: WorkflowAgentCallRequest,
    ) -> Result<WorkflowAgentCallDispatchOutcome, WorkflowAgentCallDispatchError> {
        self.dispatch_inner(request).await
    }
}

pub fn build_workflow_agent_call_dispatch(
    product: Arc<dyn WorkflowAgentCallProductPort>,
    product_launch: Arc<AgentRunProductLaunchService>,
    product_input_delivery: Arc<dyn AgentRunProductInputDeliveryPort>,
) -> Arc<dyn WorkflowAgentCallDispatchPort> {
    Arc::new(ProductWorkflowAgentCallDispatchService::new(
        product,
        product_launch,
        product_input_delivery,
    ))
}

fn runtime_thread_id(
    request: &WorkflowAgentCallRequest,
) -> Result<RuntimeThreadId, WorkflowAgentCallDispatchError> {
    match &request.target_intent {
        WorkflowAgentCallTargetIntent::CreateNew { .. } => RuntimeThreadId::new(
            stable_workflow_agent_call_uuid(&request.identity.request_id, "runtime").to_string(),
        ),
        WorkflowAgentCallTargetIntent::ContinueCurrent {
            runtime_thread_id, ..
        } => RuntimeThreadId::new(runtime_thread_id.clone()),
    }
    .map_err(|error| permanent("agent_call_runtime_thread_invalid", error.to_string()))
}

fn initial_context(
    request: &WorkflowAgentCallRequest,
) -> Result<ManagedRuntimeInitialContextPackage, WorkflowAgentCallDispatchError> {
    let payload_digest = RuntimePayloadDigest::new(request.payload_digest.clone())
        .map_err(|error| permanent("agent_call_context_invalid", error.to_string()))?;
    let provenance = ManagedRuntimeContextProvenance {
        authority: ManagedRuntimeContextAuthority::Workflow,
        source: RuntimeContextSourceRef::new(format!(
            "workflow-agent-call:{}",
            request.identity.request_id
        ))
        .map_err(|error| permanent("agent_call_context_invalid", error.to_string()))?,
        revision: RuntimeContextSourceRevision::new(request.payload_digest.clone())
            .map_err(|error| permanent("agent_call_context_invalid", error.to_string()))?,
        digest: payload_digest,
    };
    let mut contribution = ManagedRuntimeInitialContextContribution {
        contribution_id: RuntimeContextContributionId::new("workflow-agent-call-procedure")
            .map_err(|error| permanent("agent_call_context_invalid", error.to_string()))?,
        digest: RuntimePayloadDigest::new("pending").expect("non-empty digest placeholder"),
        content: ManagedRuntimeInitialContextContributionContent::WorkflowContext {
            schema: "agentdash.workflow.agent-call.procedure.v1".to_owned(),
            value: serde_json::json!({
                "procedure_key": request.procedure_key,
                "contract": request.procedure_contract,
                "target": request.target_intent.target(),
            }),
            provenance,
        },
    };
    contribution.digest = contribution.calculated_digest();
    let mut package = ManagedRuntimeInitialContextPackage {
        package_id: RuntimeContextPackageId::new(format!(
            "workflow-agent-call-context:{}",
            request.identity.request_id
        ))
        .map_err(|error| permanent("agent_call_context_invalid", error.to_string()))?,
        schema_version: 1,
        mode: ManagedRuntimeInitialContextMode::WorkflowOnly,
        contributions: vec![contribution],
        digest: RuntimePayloadDigest::new("pending").expect("non-empty digest placeholder"),
    };
    package.digest = package.calculated_digest();
    Ok(package)
}

fn workflow_input_blocks(
    input: &[WorkflowAgentCallContentBlock],
) -> Vec<agentdash_agent_service_api::AgentInputContent> {
    input
        .iter()
        .map(|block| match block {
            WorkflowAgentCallContentBlock::Text { text } => {
                agentdash_agent_service_api::AgentInputContent::Text { text: text.clone() }
            }
            WorkflowAgentCallContentBlock::Structured { schema, value } => {
                agentdash_agent_service_api::AgentInputContent::Structured {
                    schema: schema.clone(),
                    value: value.clone(),
                }
            }
        })
        .collect()
}

fn stable_workflow_agent_call_uuid(request_id: &str, role: &str) -> uuid::Uuid {
    let digest =
        Sha256::digest(format!("agentdash.workflow-agent-call/v1:{request_id}:{role}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes)
}

fn permanent(
    code: impl Into<String>,
    message: impl Into<String>,
) -> WorkflowAgentCallDispatchError {
    WorkflowAgentCallDispatchError::new(code, message, false)
}

fn retryable(
    code: impl Into<String>,
    message: impl Into<String>,
) -> WorkflowAgentCallDispatchError {
    WorkflowAgentCallDispatchError::new(code, message, true)
}

fn product_unavailable(message: String) -> WorkflowAgentCallDispatchError {
    retryable("agent_call_product_unavailable", message)
}
