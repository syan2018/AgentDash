use std::sync::Arc;

use agentdash_agent_runtime::project_authoritative_agent_snapshot;
use agentdash_agent_runtime_contract::{
    ManagedRuntimeContentBlock, ManagedRuntimeContextAuthority, ManagedRuntimeContextProvenance,
    ManagedRuntimeInitialContextContribution, ManagedRuntimeInitialContextContributionContent,
    ManagedRuntimeInitialContextMode, ManagedRuntimeInitialContextPackage, ManagedRuntimeSnapshot,
    RuntimeContextContributionId, RuntimeContextPackageId, RuntimeContextSourceRef,
    RuntimeContextSourceRevision, RuntimePayloadDigest, RuntimeProjectionRevision, RuntimeThreadId,
};
use agentdash_agent_service_api::{
    AgentEffectIdentity, AgentForkPoint, AgentReadQuery, AgentReceiptState, AgentTurnId,
    AppliedInitialContextEvidence, InitialContextDeliveryFidelity,
};
use agentdash_application_ports::agent_frame_materialization::{
    AgentRunFrameConstructionPort, FrameConstructionCommand,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentRunLineage, LifecycleAgent, LifecycleAgentRepository, LifecycleRun,
    LifecycleRunRepository,
};
use async_trait::async_trait;
use serde_json::json;
use thiserror::Error;

use crate::agent_run::{
    AgentRunCompleteAgentResolverPort, AgentRunProductLaunchRequest, AgentRunProductLaunchService,
    AgentRunProductRuntimeBindingRepository, AgentRunProductRuntimeProvisioningRequest,
    ProductAgentFrameRef, ProductAgentSurfaceFacts,
};

use super::{
    AcceptedRuntimeOperation, AgentRunForkChildProductSelection, AgentRunForkGraph,
    AgentRunForkOperationIdentity, AgentRunForkProductGraphPort, AgentRunForkRuntimeOperation,
    AgentRunForkRuntimePort, AgentRunForkSaga, AgentRunRuntimeSnapshotPort,
    CompanionFreshEffectEvidence, CompanionFreshEffectOutcome, CompanionFreshOperation,
    CompanionFreshOperationIdentity, CompanionFreshRuntimePort, CompanionFreshSaga,
    CompanionRuntimePreparation, CompiledContextApplication, CompiledContextAuthority,
    CompiledContextContributionApplication, CompiledContextDeliveryFidelity,
    CompiledFreshContextMode, CompiledInitialContextContribution, CompiledInitialContextPackage,
    PreparedAgentRunForkGraph, RuntimeForkPhaseEvidence, RuntimeOperationOutcome,
};

/// Product flow snapshot access backed directly by the associated concrete Agent.
pub struct ProductAgentRunRuntimeSnapshotAdapter {
    bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    agents: Arc<dyn AgentRunCompleteAgentResolverPort>,
}

impl ProductAgentRunRuntimeSnapshotAdapter {
    pub fn new(
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        agents: Arc<dyn AgentRunCompleteAgentResolverPort>,
    ) -> Self {
        Self { bindings, agents }
    }
}

#[async_trait]
impl AgentRunRuntimeSnapshotPort for ProductAgentRunRuntimeSnapshotAdapter {
    async fn load_snapshot(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<ManagedRuntimeSnapshot, String> {
        let binding = self
            .bindings
            .load_product_binding_by_runtime_thread(thread_id)
            .await?
            .ok_or_else(|| format!("Product binding for Runtime thread {thread_id} is missing"))?;
        let resolved = self.agents.resolve(&binding).await?;
        let snapshot = resolved
            .service
            .read(AgentReadQuery {
                source: binding.agent.source,
                at_revision: None,
            })
            .await
            .map_err(|error| error.to_string())?;
        project_authoritative_agent_snapshot(thread_id.clone(), snapshot)
            .map_err(|error| error.to_string())
    }
}

/// Direct concrete-Agent fork protocol adapter.
///
/// The saga persists the stable Product association and Agent-owned history digest. Runtime and
/// Host state are used only to attach the current process route.
pub struct ProductAgentRunForkRuntimeAdapter {
    product_launch: Arc<AgentRunProductLaunchService>,
}

impl ProductAgentRunForkRuntimeAdapter {
    pub fn with_product_launch(product_launch: Arc<AgentRunProductLaunchService>) -> Self {
        Self { product_launch }
    }

    fn accepted(identity: &AgentRunForkOperationIdentity) -> AcceptedRuntimeOperation {
        AcceptedRuntimeOperation {
            operation_id: identity.runtime_operation_id.clone(),
            accepted_revision: RuntimeProjectionRevision(0),
        }
    }

    async fn apply(
        &self,
        saga: &AgentRunForkSaga,
        identity: &AgentRunForkOperationIdentity,
    ) -> Result<RuntimeOperationOutcome, String> {
        let receipt = Self::accepted(identity);
        match identity.operation {
            AgentRunForkRuntimeOperation::Fork => {
                let evidence = self
                    .product_launch
                    .fork_agent_source(
                        &AgentRunTarget {
                            run_id: saga.parent().run_id,
                            agent_id: saga.parent().agent_id,
                        },
                        &saga.child().runtime_thread_id,
                        AgentForkPoint::CompletedTurn {
                            turn_id: AgentTurnId::new(saga.parent().through_turn_id.as_str())
                                .map_err(|error| error.to_string())?,
                        },
                        AgentEffectIdentity::new(identity.runtime_operation_id.as_str())
                            .map_err(|error| error.to_string())?,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                if !matches!(
                    evidence.receipt.state,
                    AgentReceiptState::Terminal { .. } | AgentReceiptState::AlreadyApplied { .. }
                ) {
                    return Ok(RuntimeOperationOutcome::Unknown);
                }
                Ok(RuntimeOperationOutcome::Applied(
                    RuntimeForkPhaseEvidence::ForkProvisioned {
                        child_thread_id: saga.child().runtime_thread_id.clone(),
                        child_binding: evidence.association,
                        child_history_digest: RuntimePayloadDigest::new(
                            evidence.child_history_digest.as_str(),
                        )
                        .map_err(|error| error.to_string())?,
                        context: None,
                        receipt,
                    },
                ))
            }
            AgentRunForkRuntimeOperation::Rebind => {
                let request = saga.materialized_child_product_selection().ok_or_else(|| {
                    "fork child Product binding has not been materialized".to_owned()
                })?;
                let association = saga
                    .child_binding()
                    .cloned()
                    .ok_or_else(|| "fork child Agent association is missing".to_owned())?;
                let binding = self
                    .product_launch
                    .bind_forked_agent_source(request, association)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(RuntimeOperationOutcome::Applied(
                    RuntimeForkPhaseEvidence::Rebound {
                        child_thread_id: saga.child().runtime_thread_id.clone(),
                        child_binding: binding.agent,
                        receipt,
                    },
                ))
            }
            AgentRunForkRuntimeOperation::Activate => {
                let association = saga
                    .child_binding()
                    .cloned()
                    .ok_or_else(|| "fork child Agent association is missing".to_owned())?;
                Ok(RuntimeOperationOutcome::Applied(
                    RuntimeForkPhaseEvidence::Activated {
                        child_thread_id: saga.child().runtime_thread_id.clone(),
                        child_binding: association,
                        context: saga.initial_context_evidence().cloned(),
                        receipt,
                    },
                ))
            }
        }
    }
}

#[async_trait]
impl AgentRunForkRuntimePort for ProductAgentRunForkRuntimeAdapter {
    async fn execute(
        &self,
        saga: &AgentRunForkSaga,
        identity: &AgentRunForkOperationIdentity,
    ) -> Result<RuntimeOperationOutcome, String> {
        self.apply(saga, identity).await
    }

    async fn inspect(
        &self,
        saga: &AgentRunForkSaga,
        identity: &AgentRunForkOperationIdentity,
    ) -> Result<RuntimeOperationOutcome, String> {
        self.apply(saga, identity).await
    }
}

/// Direct concrete-Agent fresh Companion protocol adapter.
pub struct ProductCompanionFreshRuntimeAdapter {
    product_launch: Arc<AgentRunProductLaunchService>,
}

impl ProductCompanionFreshRuntimeAdapter {
    pub fn with_product_launch(product_launch: Arc<AgentRunProductLaunchService>) -> Self {
        Self { product_launch }
    }

    fn initial_context(
        saga: &CompanionFreshSaga,
    ) -> Result<&CompiledInitialContextPackage, String> {
        match &saga.plan().preparation {
            CompanionRuntimePreparation::FreshCreate { initial_context } => Ok(initial_context),
            CompanionRuntimePreparation::ForkParentHistory { .. } => {
                Err("fresh Companion saga does not contain a fresh context package".to_owned())
            }
        }
    }

    fn accepted(
        identity: &CompanionFreshOperationIdentity,
        revision: u64,
    ) -> AcceptedRuntimeOperation {
        AcceptedRuntimeOperation {
            operation_id: identity.runtime_operation_id.clone(),
            accepted_revision: RuntimeProjectionRevision(revision),
        }
    }

    async fn apply(
        &self,
        saga: &CompanionFreshSaga,
        identity: &CompanionFreshOperationIdentity,
    ) -> Result<CompanionFreshEffectOutcome, String> {
        match identity.operation {
            CompanionFreshOperation::CreateWithContextPackage => {
                let initial_context =
                    compile_runtime_initial_context(Self::initial_context(saga)?)?;
                let outcome = self
                    .product_launch
                    .launch(AgentRunProductLaunchRequest {
                        provisioning: saga.provisioning().clone(),
                        initial_context: Some(initial_context),
                        initial_input: Vec::new(),
                    })
                    .await
                    .map_err(|error| error.to_string())?;
                let Some(applied_context) = outcome.create_receipt.initial_context.as_ref() else {
                    return Err(
                        "concrete Agent Create receipt omitted initial-context evidence".to_owned(),
                    );
                };
                let context = map_agent_initial_context_evidence(
                    Self::initial_context(saga)?,
                    applied_context,
                )?;
                Ok(CompanionFreshEffectOutcome::Applied(
                    CompanionFreshEffectEvidence::Created {
                        child_runtime_thread_id: saga.runtime_thread_id().clone(),
                        child_binding: outcome.binding.agent,
                        context,
                        receipt: Self::accepted(
                            identity,
                            outcome
                                .create_receipt
                                .snapshot_revision
                                .map_or(0, |revision| revision.0),
                        ),
                    },
                ))
            }
            CompanionFreshOperation::Activate => {
                let association = saga
                    .child_binding()
                    .cloned()
                    .ok_or_else(|| "fresh Companion Agent association is missing".to_owned())?;
                Ok(CompanionFreshEffectOutcome::Applied(
                    CompanionFreshEffectEvidence::Activated {
                        child_runtime_thread_id: saga.runtime_thread_id().clone(),
                        child_binding: association,
                        receipt: Self::accepted(identity, 0),
                    },
                ))
            }
            CompanionFreshOperation::SubmitFirstInput => {
                self.product_launch
                    .submit_input(
                        saga.provisioning().target.clone(),
                        identity.runtime_operation_id.as_str().to_owned(),
                        vec![ManagedRuntimeContentBlock::Text {
                            text: saga.plan().first_submit_input.text.clone(),
                        }],
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(CompanionFreshEffectOutcome::Applied(
                    CompanionFreshEffectEvidence::FirstInputSubmitted {
                        child_runtime_thread_id: saga.runtime_thread_id().clone(),
                        receipt: Self::accepted(identity, 0),
                    },
                ))
            }
        }
    }
}

#[async_trait]
impl CompanionFreshRuntimePort for ProductCompanionFreshRuntimeAdapter {
    async fn execute(
        &self,
        saga: &CompanionFreshSaga,
        identity: &CompanionFreshOperationIdentity,
    ) -> Result<CompanionFreshEffectOutcome, String> {
        self.apply(saga, identity).await
    }

    async fn inspect(
        &self,
        saga: &CompanionFreshSaga,
        identity: &CompanionFreshOperationIdentity,
    ) -> Result<CompanionFreshEffectOutcome, String> {
        self.apply(saga, identity).await
    }
}

fn map_agent_initial_context_evidence(
    product_package: &CompiledInitialContextPackage,
    evidence: &AppliedInitialContextEvidence,
) -> Result<CompiledContextApplication, String> {
    let runtime_package = compile_runtime_initial_context(product_package)?;
    let agent_package = agentdash_agent_runtime::map_initial_context_package(runtime_package)
        .map_err(|error| error.to_string())?;
    if evidence.package_id != agent_package.package_id
        || evidence.package_digest != agent_package.digest
    {
        return Err("concrete Agent initial-context evidence drifted".to_owned());
    }
    let fidelity = match evidence.fidelity {
        InitialContextDeliveryFidelity::TypedNative => CompiledContextDeliveryFidelity::TypedNative,
        InitialContextDeliveryFidelity::CanonicalRendered => {
            CompiledContextDeliveryFidelity::CanonicalRendered
        }
        InitialContextDeliveryFidelity::Unsupported => CompiledContextDeliveryFidelity::Unsupported,
    };
    Ok(CompiledContextApplication {
        package_id: product_package.package_id,
        package_digest: product_package.digest.clone(),
        fidelity,
        contribution_fidelity: product_package
            .contributions
            .iter()
            .map(|contribution| CompiledContextContributionApplication {
                kind: contribution.kind_name().to_owned(),
                fidelity,
            })
            .collect(),
        renderer_version: evidence.renderer_version.clone(),
        materialized_digest: evidence
            .materialized_digest
            .as_ref()
            .map(|digest| digest.as_str().to_owned()),
    })
}

fn compile_runtime_initial_context(
    package: &CompiledInitialContextPackage,
) -> Result<ManagedRuntimeInitialContextPackage, String> {
    if !package.digest_matches() {
        return Err("Product initial-context package digest is invalid".to_owned());
    }
    let schema_version = u32::try_from(package.schema_version)
        .map_err(|_| "initial-context schema version exceeds Runtime contract".to_owned())?;
    let package_id = RuntimeContextPackageId::new(package.package_id.to_string())
        .map_err(|error| error.to_string())?;
    let mut contributions = Vec::with_capacity(package.contributions.len());
    for (index, contribution) in package.contributions.iter().enumerate() {
        let contribution_id = RuntimeContextContributionId::new(format!(
            "{}:{}:{index}",
            package.package_id,
            contribution.kind_name()
        ))
        .map_err(|error| error.to_string())?;
        let content = compile_runtime_contribution(contribution)?;
        let mut contribution = ManagedRuntimeInitialContextContribution {
            contribution_id,
            digest: RuntimePayloadDigest::new("pending").map_err(|error| error.to_string())?,
            content,
        };
        contribution.digest = contribution.calculated_digest();
        contributions.push(contribution);
    }
    let mut runtime_package = ManagedRuntimeInitialContextPackage {
        package_id,
        schema_version,
        mode: match package.mode {
            CompiledFreshContextMode::Compact => ManagedRuntimeInitialContextMode::Compact,
            CompiledFreshContextMode::WorkflowOnly => {
                ManagedRuntimeInitialContextMode::WorkflowOnly
            }
            CompiledFreshContextMode::ConstraintsOnly => {
                ManagedRuntimeInitialContextMode::ConstraintsOnly
            }
        },
        contributions,
        digest: RuntimePayloadDigest::new("pending").map_err(|error| error.to_string())?,
    };
    runtime_package.digest = runtime_package.calculated_digest();
    if !runtime_package.validate() {
        return Err("compiled Runtime initial-context package is invalid".to_owned());
    }
    Ok(runtime_package)
}

fn compile_runtime_contribution(
    contribution: &CompiledInitialContextContribution,
) -> Result<ManagedRuntimeInitialContextContributionContent, String> {
    Ok(match contribution {
        CompiledInitialContextContribution::CompactSummary {
            summary,
            provenance,
        } => ManagedRuntimeInitialContextContributionContent::CompactSummary {
            summary: summary.clone(),
            provenance: compile_runtime_provenance(provenance)?,
        },
        CompiledInitialContextContribution::WorkflowContext {
            payload,
            provenance,
        } => ManagedRuntimeInitialContextContributionContent::WorkflowContext {
            schema: payload.schema.clone(),
            value: payload.value.clone(),
            provenance: compile_runtime_provenance(provenance)?,
        },
        CompiledInitialContextContribution::ConstraintSet {
            payload,
            provenance,
        } => ManagedRuntimeInitialContextContributionContent::ConstraintSet {
            schema: payload.schema.clone(),
            value: payload.value.clone(),
            provenance: compile_runtime_provenance(provenance)?,
        },
    })
}

fn compile_runtime_provenance(
    provenance: &super::CompiledContextProvenance,
) -> Result<ManagedRuntimeContextProvenance, String> {
    Ok(ManagedRuntimeContextProvenance {
        authority: match provenance.authority {
            CompiledContextAuthority::AgentHistory => ManagedRuntimeContextAuthority::AgentHistory,
            CompiledContextAuthority::AgentSnapshot => {
                ManagedRuntimeContextAuthority::AgentSnapshot
            }
            CompiledContextAuthority::Workflow => ManagedRuntimeContextAuthority::Workflow,
            CompiledContextAuthority::Constraint => ManagedRuntimeContextAuthority::Constraint,
        },
        source: RuntimeContextSourceRef::new(provenance.source.clone())
            .map_err(|error| error.to_string())?,
        revision: RuntimeContextSourceRevision::new(provenance.revision.clone())
            .map_err(|error| error.to_string())?,
        digest: RuntimePayloadDigest::new(provenance.digest.clone())
            .map_err(|error| error.to_string())?,
    })
}

#[derive(Debug, Error)]
enum ProductAgentRunForkGraphError {
    #[error("parent LifecycleRun {0} was not found")]
    ParentRunNotFound(uuid::Uuid),
    #[error("parent LifecycleAgent {0} was not found")]
    ParentAgentNotFound(uuid::Uuid),
    #[error("parent AgentFrame for agent {0} was not found")]
    ParentFrameNotFound(uuid::Uuid),
    #[error("parent Product graph is inconsistent: {0}")]
    InconsistentParent(&'static str),
    #[error("Product repository failed while reading {entity}: {reason}")]
    Repository {
        entity: &'static str,
        reason: String,
    },
    #[error("prepared Product graph is invalid: {0}")]
    InvalidPreparedGraph(String),
}

/// Product-owned production adapter that prepares the immutable child graph for a fork.
///
/// This adapter is deliberately read-only. The prepared graph is published only by
/// `AgentRunForkSagaRepository::commit_product_graph`, which owns the atomic graph + saga
/// transition required by the fork protocol.
pub struct ProductAgentRunForkGraphAdapter {
    runs: Arc<dyn LifecycleRunRepository>,
    agents: Arc<dyn LifecycleAgentRepository>,
    frames: Arc<dyn AgentFrameRepository>,
    frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
}

impl ProductAgentRunForkGraphAdapter {
    pub fn new(
        runs: Arc<dyn LifecycleRunRepository>,
        agents: Arc<dyn LifecycleAgentRepository>,
        frames: Arc<dyn AgentFrameRepository>,
        frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
    ) -> Self {
        Self {
            runs,
            agents,
            frames,
            frame_construction,
        }
    }

    async fn prepare(
        &self,
        saga: &AgentRunForkSaga,
    ) -> Result<PreparedAgentRunForkGraph, ProductAgentRunForkGraphError> {
        let parent = saga.parent();
        let parent_run = self
            .runs
            .get_by_id(parent.run_id)
            .await
            .map_err(|error| ProductAgentRunForkGraphError::Repository {
                entity: "LifecycleRun",
                reason: error.to_string(),
            })?
            .ok_or(ProductAgentRunForkGraphError::ParentRunNotFound(
                parent.run_id,
            ))?;
        let parent_agent = self
            .agents
            .get(parent.agent_id)
            .await
            .map_err(|error| ProductAgentRunForkGraphError::Repository {
                entity: "LifecycleAgent",
                reason: error.to_string(),
            })?
            .ok_or(ProductAgentRunForkGraphError::ParentAgentNotFound(
                parent.agent_id,
            ))?;
        let parent_frame = self
            .frames
            .get_latest(parent.agent_id)
            .await
            .map_err(|error| ProductAgentRunForkGraphError::Repository {
                entity: "AgentFrame",
                reason: error.to_string(),
            })?
            .ok_or(ProductAgentRunForkGraphError::ParentFrameNotFound(
                parent.agent_id,
            ))?;

        if parent_agent.run_id != parent_run.id {
            return Err(ProductAgentRunForkGraphError::InconsistentParent(
                "LifecycleAgent does not belong to LifecycleRun",
            ));
        }
        if parent_agent.project_id != parent_run.project_id {
            return Err(ProductAgentRunForkGraphError::InconsistentParent(
                "LifecycleAgent and LifecycleRun have different projects",
            ));
        }
        if parent_frame.agent_id != parent_agent.id {
            return Err(ProductAgentRunForkGraphError::InconsistentParent(
                "AgentFrame does not belong to LifecycleAgent",
            ));
        }

        let child = saga.child();
        let product_intent = saga.product_intent();
        let requested_by_user_id = product_intent
            .map(|intent| intent.requested_by_user_id.as_str())
            .unwrap_or(parent_agent.created_by_user_id.as_str());
        let requested_at = product_intent
            .map(|intent| intent.requested_at)
            .unwrap_or(parent_run.last_activity_at);
        let mut child_run = LifecycleRun::new_plain_for_user(
            parent_run.project_id,
            requested_by_user_id.to_owned(),
        );
        child_run.id = child.run_id;
        child_run.created_at = requested_at;
        child_run.updated_at = requested_at;
        child_run.last_activity_at = requested_at;

        let mut child_agent = LifecycleAgent::new_root_for_user(
            child.run_id,
            parent_run.project_id,
            parent_agent.source,
            requested_by_user_id.to_owned(),
        );
        child_agent.id = child.agent_id;
        child_agent.project_agent_id = saga
            .child_product_selection()
            .map(|selection| selection.project_agent_id)
            .or(parent_agent.project_agent_id);
        child_agent.bootstrap_status = parent_agent.bootstrap_status.clone();
        child_agent.workspace_title = product_intent
            .and_then(|intent| intent.title.clone())
            .or_else(|| parent_agent.workspace_title.clone());
        child_agent.workspace_title_source = parent_agent
            .workspace_title
            .as_ref()
            .map(|_| "source".to_owned());
        if product_intent.is_some_and(|intent| intent.title.is_some()) {
            child_agent.workspace_title_source = Some("user".to_owned());
        }
        child_agent.created_at = requested_at;
        child_agent.updated_at = requested_at;

        let mut child_frame = if saga.child_product_selection().is_some() {
            agentdash_domain::workflow::AgentFrame::new_revision(
                child.agent_id,
                1,
                "agent_run_fork_selection_pending",
            )
        } else {
            let mut inherited = parent_frame.clone();
            inherited.agent_id = child.agent_id;
            inherited.revision = 1;
            inherited.created_by_kind = "agent_run_fork_product_protocol".to_owned();
            inherited
        };
        child_frame.id = child.frame_id;
        child_frame.created_by_id = Some(requested_by_user_id.to_owned());
        child_frame.created_at = requested_at;

        let mut lineage = AgentRunLineage::new_fork(
            parent.run_id,
            parent.agent_id,
            child.run_id,
            child.agent_id,
            None,
            Some(match product_intent {
                Some(intent) => json!({
                    "turn_id": intent.source_turn_id,
                    "entry_index": intent.source_entry_index,
                }),
                None => json!({
                    "kind": "completed_turn",
                    "runtime_thread_id": parent.runtime_thread_id,
                    "turn_id": parent.through_turn_id,
                }),
            }),
            requested_by_user_id,
            product_intent
                .and_then(|intent| intent.metadata_json.clone())
                .or_else(|| {
                    Some(json!({
                        "agent_run_id": child.agent_run_id,
                        "runtime_thread_id": child.runtime_thread_id,
                    }))
                }),
        )
        .with_frame_baseline(
            parent_frame.id,
            parent_frame.revision,
            child.frame_id,
            child_frame.revision,
        );
        lineage.id = saga.request_id().0;
        lineage.created_at = requested_at;

        PreparedAgentRunForkGraph::prepare(
            saga,
            AgentRunForkGraph {
                child_run,
                child_agent,
                child_frame,
                lineage,
            },
        )
        .map_err(|error| ProductAgentRunForkGraphError::InvalidPreparedGraph(error.to_string()))
    }

    fn selected_provisioning(
        saga: &AgentRunForkSaga,
        selection: &AgentRunForkChildProductSelection,
        frame: &agentdash_domain::workflow::AgentFrame,
    ) -> Result<AgentRunProductRuntimeProvisioningRequest, String> {
        let child = saga.child();
        if frame.agent_id != child.agent_id
            || frame.id != selection.materialized_frame_id
            || frame.created_by_kind != "dispatch_launch_anchor"
            || frame.execution_profile_json.as_ref()
                != Some(&selection.execution_profile.configuration)
        {
            return Err("materialized selected child frame evidence drifted".to_owned());
        }
        let revision = u64::try_from(frame.revision)
            .map_err(|_| "materialized child frame revision is invalid".to_owned())?;
        Ok(AgentRunProductRuntimeProvisioningRequest {
            target: agentdash_domain::agent_run_target::AgentRunTarget {
                run_id: child.run_id,
                agent_id: child.agent_id,
            },
            runtime_thread_id: child.runtime_thread_id.clone(),
            idempotency_key: selection.idempotency_key.clone(),
            frame: ProductAgentFrameRef {
                frame_id: frame.id,
                agent_id: frame.agent_id,
                revision,
            },
            execution_profile: selection.execution_profile.clone(),
            surface_facts: ProductAgentSurfaceFacts::from_frame(frame),
        })
    }
}

#[async_trait]
impl AgentRunForkProductGraphPort for ProductAgentRunForkGraphAdapter {
    async fn prepare_child_graph_commit(
        &self,
        saga: &AgentRunForkSaga,
    ) -> Result<PreparedAgentRunForkGraph, String> {
        self.prepare(saga).await.map_err(|error| error.to_string())
    }

    async fn materialize_child_product_selection(
        &self,
        saga: &AgentRunForkSaga,
    ) -> Result<AgentRunProductRuntimeProvisioningRequest, String> {
        let child = saga.child();
        let selection = saga
            .child_product_selection()
            .ok_or_else(|| "fork child Product selection is missing".to_owned())?;
        let child_agent = self
            .agents
            .get(child.agent_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "fork child LifecycleAgent is not committed".to_owned())?;
        if child_agent.project_agent_id != Some(selection.project_agent_id) {
            return Err("fork child selected ProjectAgent drifted".to_owned());
        }
        if let Some(existing) = self
            .frames
            .get(selection.materialized_frame_id)
            .await
            .map_err(|error| error.to_string())?
        {
            return Self::selected_provisioning(saga, selection, &existing);
        }
        let outcome = self
            .frame_construction
            .execute_frame_construction_command(FrameConstructionCommand::DispatchLaunchAnchor {
                run_id: child.run_id,
                agent_id: child.agent_id,
                target_frame_id: Some(selection.materialized_frame_id),
                subject_ref: None,
                runtime_thread_id: Some(child.runtime_thread_id.to_string()),
                created_by_id: Some(child_agent.created_by_user_id.clone()),
                execution_profile: Some(selection.execution_profile.configuration.clone()),
            })
            .await
            .map_err(|error| error.to_string())?;
        let frame_id = outcome.frame_id.ok_or_else(|| {
            "selected child frame construction returned no frame identity".to_owned()
        })?;
        let frame = self
            .frames
            .get(frame_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "selected child frame was not durably materialized".to_owned())?;
        Self::selected_provisioning(saga, selection, &frame)
    }
}
