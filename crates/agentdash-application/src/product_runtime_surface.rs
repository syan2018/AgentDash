use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceProvenance,
    AgentRunAppliedResourceSurfaceQueryError, AgentRunAppliedResourceSurfaceQueryPort,
    AgentRunProductRuntimeBinding, AgentRunProductRuntimeBindingRepository, AppliedTaskGrant,
    AppliedTaskOperation, AppliedTaskScope, AppliedVfsGrant, AppliedVfsMount, AppliedVfsOperation,
    AppliedVfsPathScope, ProductAgentSurfaceFacts,
};
use agentdash_application_lifecycle::{
    AgentRunLifecycleMountFacts, AgentRunLifecycleMountFactsQueryPort,
};
use agentdash_application_ports::lifecycle_surface_projection::lifecycle_identity_from_orchestration;
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::common::MountCapability;
use agentdash_domain::workflow::{
    AgentFrame, LifecycleAgent, LifecycleRun, OrchestrationInstance, PlanNode, RuntimeNodeState,
    RuntimeNodeStatus,
};
use agentdash_platform_spi::Vfs;
use async_trait::async_trait;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::repository_set::RepositorySet;

#[derive(Clone)]
pub struct ProductAgentRunFactsResolver {
    repos: RepositorySet,
    bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
}

struct ResolvedProductAgentRunFacts {
    binding: AgentRunProductRuntimeBinding,
    binding_digest: String,
    run: LifecycleRun,
    agent: LifecycleAgent,
    frame: AgentFrame,
    vfs: Vfs,
    workspace_id: Option<uuid::Uuid>,
    node: Option<ResolvedWorkflowNodeFacts>,
    skill_asset_keys: Vec<String>,
}

struct ResolvedWorkflowNodeFacts {
    orchestration_id: uuid::Uuid,
    node_path: String,
    attempt: u32,
    lifecycle_key: String,
    writable_port_keys: Vec<String>,
}

impl ProductAgentRunFactsResolver {
    pub fn new(
        repos: RepositorySet,
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    ) -> Self {
        Self { repos, bindings }
    }

    async fn resolve(
        &self,
        target: &AgentRunTarget,
    ) -> Result<ResolvedProductAgentRunFacts, AgentRunAppliedResourceSurfaceQueryError> {
        let binding = self
            .bindings
            .load_product_binding(target)
            .await
            .map_err(repository)?
            .ok_or(AgentRunAppliedResourceSurfaceQueryError::MissingFacts)?;
        if binding.target != *target || binding.launch_frame.agent_id != target.agent_id {
            return Err(conflict("Product binding target/frame mismatch"));
        }
        let binding_digest = binding.calculated_digest().map_err(repository)?;
        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(target.run_id)
            .await
            .map_err(|error| repository(error.to_string()))?
            .ok_or(AgentRunAppliedResourceSurfaceQueryError::MissingFacts)?;
        let agent = self
            .repos
            .lifecycle_agent_repo
            .get(target.agent_id)
            .await
            .map_err(|error| repository(error.to_string()))?
            .ok_or(AgentRunAppliedResourceSurfaceQueryError::MissingFacts)?;
        if agent.run_id != run.id
            || agent.project_id != run.project_id
            || agent.id != target.agent_id
        {
            return Err(conflict(
                "Lifecycle AgentRun facts do not match Product target",
            ));
        }
        let launch_frame = self
            .repos
            .agent_frame_repo
            .get(binding.launch_frame.frame_id)
            .await
            .map_err(|error| repository(error.to_string()))?
            .ok_or(AgentRunAppliedResourceSurfaceQueryError::MissingFacts)?;
        if launch_frame.agent_id != agent.id
            || u64::try_from(launch_frame.revision).ok() != Some(binding.launch_frame.revision)
        {
            return Err(conflict(
                "immutable launch AgentFrame does not match Product binding",
            ));
        }
        let frame = launch_frame;
        let vfs = frame
            .surface_document()
            .vfs_surface
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        let project = self
            .repos
            .project_repo
            .get_by_id(run.project_id)
            .await
            .map_err(|error| repository(error.to_string()))?
            .ok_or(AgentRunAppliedResourceSurfaceQueryError::MissingFacts)?;
        let node = active_workflow_node(&run, target);
        let skill_asset_keys = lifecycle_skill_asset_keys(&vfs);
        Ok(ResolvedProductAgentRunFacts {
            binding,
            binding_digest,
            run,
            agent,
            frame,
            vfs,
            workspace_id: project.config.default_workspace_id,
            node,
            skill_asset_keys,
        })
    }
}

pub struct ProductAgentRunAppliedResourceSurfaceCompiler {
    facts: ProductAgentRunFactsResolver,
}

impl ProductAgentRunAppliedResourceSurfaceCompiler {
    pub fn new(facts: ProductAgentRunFactsResolver) -> Self {
        Self { facts }
    }
}

#[async_trait]
impl AgentRunAppliedResourceSurfaceQueryPort for ProductAgentRunAppliedResourceSurfaceCompiler {
    async fn applied_resource_surface(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryError> {
        let facts = self.facts.resolve(target).await?;
        let candidate_default_mount_id = facts.vfs.default_mount_id.clone();
        let mut mounts = Vec::new();
        let mut grants = Vec::new();
        for mount in facts.vfs.mounts.iter().cloned() {
            let capabilities = mount
                .capabilities
                .iter()
                .filter_map(applied_vfs_operation)
                .collect::<BTreeSet<_>>();
            if capabilities.is_empty() {
                continue;
            }
            grants.push(AppliedVfsGrant {
                mount_id: mount.id.clone(),
                operations: capabilities.clone(),
                path_scopes: vec![AppliedVfsPathScope::All],
            });
            mounts.push(AppliedVfsMount {
                mount_id: mount.id,
                provider: mount.provider,
                backend_id: mount.backend_id,
                root_ref: mount.root_ref,
                capabilities,
                default_write: mount.default_write,
                display_name: mount.display_name,
                metadata: mount.metadata,
            });
        }
        let default_mount_id = candidate_default_mount_id
            .filter(|id| mounts.iter().any(|mount| &mount.mount_id == id));
        let task_grants = product_task_grants(&self.facts.repos, &facts).await?;
        let surface_facts = ProductAgentSurfaceFacts::from_frame(&facts.frame);
        let captured_at_ms =
            u64::try_from(facts.frame.created_at.timestamp_millis()).unwrap_or_default();
        let provenance = AgentRunAppliedResourceSurfaceProvenance {
            source_kind: "agent_frame".to_string(),
            source_id: facts.frame.id.to_string(),
            source_revision: u64::try_from(facts.frame.revision).unwrap_or_default(),
            projection_revision: surface_facts.surface_revision,
            captured_at_ms,
        };
        let task_digest = digest(&("agentdash.product-task-grants/v1", &task_grants))?;
        let vfs_digest = digest(&(
            "agentdash.product-vfs-grants/v1",
            &mounts,
            &default_mount_id,
            &grants,
        ))?;
        let surface = AgentRunAppliedResourceSurface {
            target: target.clone(),
            project_id: facts.run.project_id,
            workspace_id: facts.workspace_id,
            vfs_mounts: mounts,
            default_mount_id,
            vfs_grants: grants,
            agent_surface_revision: surface_facts.surface_revision,
            agent_surface_digest: surface_facts.surface_digest,
            vfs_digest,
            task_grants,
            task_surface_digest: task_digest,
            product_binding_digest: facts.binding_digest,
            provenance,
        };
        surface.validate_for(target).map_err(|error| {
            AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                message: error.to_string(),
            }
        })?;
        Ok(surface)
    }
}

#[async_trait]
impl AgentRunLifecycleMountFactsQueryPort for ProductAgentRunFactsResolver {
    async fn lifecycle_mount_facts(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunLifecycleMountFacts, AgentRunAppliedResourceSurfaceQueryError> {
        let facts = self.resolve(target).await?;
        let (orchestration_id, node_path, node_attempt, node_lifecycle_key, writable_port_keys) =
            facts
                .node
                .map(|node| {
                    (
                        Some(node.orchestration_id),
                        Some(node.node_path),
                        Some(node.attempt),
                        Some(node.lifecycle_key),
                        node.writable_port_keys,
                    )
                })
                .unwrap_or((None, None, None, None, Vec::new()));
        Ok(AgentRunLifecycleMountFacts {
            target: target.clone(),
            runtime_thread_id: facts.binding.runtime_thread_id,
            launch_frame_id: facts.frame.id,
            product_binding_digest: facts.binding_digest,
            orchestration_id,
            node_path,
            node_attempt,
            node_lifecycle_key,
            writable_port_keys,
            skill_asset_keys: facts.skill_asset_keys,
        })
    }
}

async fn product_task_grants(
    repos: &RepositorySet,
    facts: &ResolvedProductAgentRunFacts,
) -> Result<Vec<AppliedTaskGrant>, AgentRunAppliedResourceSurfaceQueryError> {
    let mut grants = vec![AppliedTaskGrant {
        scope: AppliedTaskScope::Project {
            project_id: facts.run.project_id,
        },
        operations: BTreeSet::from([AppliedTaskOperation::Read, AppliedTaskOperation::Write]),
    }];
    for association in repos
        .lifecycle_subject_association_repo
        .list_by_anchor(facts.run.id, Some(facts.agent.id))
        .await
        .map_err(|error| repository(error.to_string()))?
    {
        if association.subject_kind == "task" {
            grants.push(AppliedTaskGrant {
                scope: AppliedTaskScope::Task {
                    project_id: facts.run.project_id,
                    task_id: association.subject_id,
                },
                operations: BTreeSet::from([
                    AppliedTaskOperation::Read,
                    AppliedTaskOperation::Write,
                ]),
            });
        }
    }
    grants.sort_by(|left, right| left.scope.cmp(&right.scope));
    grants.dedup_by(|left, right| left.scope == right.scope);
    Ok(grants)
}

fn active_workflow_node(
    run: &LifecycleRun,
    target: &AgentRunTarget,
) -> Option<ResolvedWorkflowNodeFacts> {
    run.orchestrations.iter().find_map(|orchestration| {
        let node = find_active_node(&orchestration.node_tree, target)?;
        let plan_node = orchestration.plan_snapshot.nodes.iter().find(|candidate| {
            candidate.node_path == node.node_path || candidate.node_id == node.node_id
        })?;
        Some(workflow_node_facts(orchestration, node, plan_node))
    })
}

fn workflow_node_facts(
    orchestration: &OrchestrationInstance,
    node: &RuntimeNodeState,
    plan_node: &PlanNode,
) -> ResolvedWorkflowNodeFacts {
    ResolvedWorkflowNodeFacts {
        orchestration_id: orchestration.orchestration_id,
        node_path: node.node_path.clone(),
        attempt: node.attempt,
        lifecycle_key: lifecycle_identity_from_orchestration(orchestration).key,
        writable_port_keys: plan_node
            .output_ports
            .iter()
            .map(|port| port.key.clone())
            .collect(),
    }
}

fn find_active_node<'a>(
    nodes: &'a [RuntimeNodeState],
    target: &AgentRunTarget,
) -> Option<&'a RuntimeNodeState> {
    nodes.iter().find_map(|node| {
        if node.agent_call.as_ref().is_some_and(|call| {
            call.target == *target
                && matches!(
                    node.status,
                    RuntimeNodeStatus::Ready
                        | RuntimeNodeStatus::Claiming
                        | RuntimeNodeStatus::Running
                        | RuntimeNodeStatus::Blocked
                )
        }) {
            Some(node)
        } else {
            find_active_node(&node.children, target)
        }
    })
}

fn lifecycle_skill_asset_keys(vfs: &Vfs) -> Vec<String> {
    let mut keys = vfs
        .mounts
        .iter()
        .find(|mount| mount.id == "lifecycle")
        .and_then(|mount| mount.metadata.get("skill_asset_keys"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    keys
}

fn applied_vfs_operation(capability: &MountCapability) -> Option<AppliedVfsOperation> {
    match capability {
        MountCapability::Read => Some(AppliedVfsOperation::Read),
        MountCapability::List => Some(AppliedVfsOperation::List),
        MountCapability::Search => Some(AppliedVfsOperation::Search),
        MountCapability::Write => Some(AppliedVfsOperation::Write),
        MountCapability::Exec => Some(AppliedVfsOperation::Exec),
        MountCapability::Watch => None,
    }
}

fn digest(value: &impl Serialize) -> Result<String, AgentRunAppliedResourceSurfaceQueryError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
            message: error.to_string(),
        }
    })?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn repository(message: impl Into<String>) -> AgentRunAppliedResourceSurfaceQueryError {
    AgentRunAppliedResourceSurfaceQueryError::Repository {
        message: message.into(),
    }
}

fn conflict(message: impl Into<String>) -> AgentRunAppliedResourceSurfaceQueryError {
    AgentRunAppliedResourceSurfaceQueryError::Conflict {
        message: message.into(),
    }
}
