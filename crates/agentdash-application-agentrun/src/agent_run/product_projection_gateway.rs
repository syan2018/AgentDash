use std::sync::Arc;

use agentdash_agent_runtime::project_authoritative_agent_snapshot;
use agentdash_agent_runtime_contract::{ManagedRuntimeSnapshot, RuntimeThreadId};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentLiveEventStream, AgentReadQuery, AgentServiceInstanceId,
    CompleteAgentService,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_workspace_module::workspace_module::presentation_protocol::{
    WorkspaceModulePresentationAcknowledgePort, WorkspaceModulePresentationAcknowledgeRequest,
    WorkspaceModulePresentationChange, WorkspaceModulePresentationChangePage,
    WorkspaceModulePresentationChangeSequence, WorkspaceModulePresentationRepository,
    WorkspaceModulePresentationSnapshot,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::AgentRunCompleteAgentAssociation;
use super::terminal_projection_protocol::{
    AgentRunTerminalChangePage, AgentRunTerminalChangeSequence,
    AgentRunTerminalProjectionRepository, AgentRunTerminalSnapshot,
};
use super::{ProductAgentFrameRef, ProductExecutionProfileRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunProductRuntimeBinding {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub agent: AgentRunCompleteAgentAssociation,
    pub launch_frame: ProductAgentFrameRef,
    pub execution_profile: ProductExecutionProfileRef,
    pub execution_profile_digest: String,
}

impl AgentRunProductRuntimeBinding {
    pub fn calculated_digest(&self) -> Result<String, String> {
        if !self.execution_profile.validate()
            || self.execution_profile.profile_digest != self.execution_profile_digest
        {
            return Err("Product Runtime binding execution profile snapshot is invalid".to_owned());
        }
        let value = serde_json::json!({
            "schema": "agentdash.agent-run-product-runtime-binding/v1",
            "target": {
                "run_id": self.target.run_id,
                "agent_id": self.target.agent_id,
            },
            "runtime_thread_id": self.runtime_thread_id,
            "agent": self.agent,
            "launch_frame": self.launch_frame,
            "execution_profile": self.execution_profile,
            "execution_profile_digest": self.execution_profile_digest,
        });
        agentdash_agent_runtime_contract::canonical_json_sha256(&value)
            .map_err(|error| error.to_string())
    }

    pub fn committed_receipt(&self) -> Result<AgentRunCommittedProductRuntimeBinding, String> {
        Ok(AgentRunCommittedProductRuntimeBinding {
            target: self.target.clone(),
            runtime_thread_id: self.runtime_thread_id.clone(),
            binding_digest: self.calculated_digest()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunCommittedProductRuntimeBinding {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub binding_digest: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentRunProductRuntimeSnapshotObservation {
    Absent {
        requested_target: AgentRunTarget,
    },
    Current {
        product_binding: AgentRunProductRuntimeBinding,
        snapshot: ManagedRuntimeSnapshot,
    },
}

#[async_trait]
pub trait AgentRunProductRuntimeBindingRepository: Send + Sync {
    async fn load_product_binding(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, String>;

    async fn load_product_binding_by_runtime_thread(
        &self,
        _runtime_thread_id: &RuntimeThreadId,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
        Err("Product Runtime binding repository does not support RuntimeThread lookup".to_string())
    }
}

#[async_trait]
pub trait AgentRunProductRuntimeBindingStore:
    AgentRunProductRuntimeBindingRepository + Send + Sync
{
    async fn commit_product_binding(
        &self,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentRunCommittedProductRuntimeBinding, String>;

    async fn replace_product_binding(
        &self,
        expected_previous_binding_digest: &str,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentRunCommittedProductRuntimeBinding, String>;
}

pub struct AgentRunProductProjectionGateway {
    runtime_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    agents: Arc<dyn AgentRunCompleteAgentResolverPort>,
    workspace_presentations: Arc<dyn WorkspaceModulePresentationRepository>,
    workspace_presentation_acknowledgements: Arc<dyn WorkspaceModulePresentationAcknowledgePort>,
    terminals: Arc<dyn AgentRunTerminalProjectionRepository>,
}

#[async_trait]
pub trait AgentRunCompleteAgentResolverPort: Send + Sync {
    async fn resolve(
        &self,
        service_instance_id: &AgentServiceInstanceId,
    ) -> Result<Arc<dyn CompleteAgentService>, String>;

    async fn binding_generation(
        &self,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentBindingGeneration, String>;
}

impl AgentRunProductProjectionGateway {
    pub fn new(
        runtime_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        agents: Arc<dyn AgentRunCompleteAgentResolverPort>,
        workspace_presentations: Arc<dyn WorkspaceModulePresentationRepository>,
        workspace_presentation_acknowledgements: Arc<
            dyn WorkspaceModulePresentationAcknowledgePort,
        >,
        terminals: Arc<dyn AgentRunTerminalProjectionRepository>,
    ) -> Self {
        Self {
            runtime_bindings,
            agents,
            workspace_presentations,
            workspace_presentation_acknowledgements,
            terminals,
        }
    }

    async fn binding(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunProductRuntimeBinding, AgentRunProductProjectionError> {
        let binding = self
            .runtime_bindings
            .load_product_binding(target)
            .await
            .map_err(AgentRunProductProjectionError::Binding)?
            .ok_or(AgentRunProductProjectionError::TargetNotBound)?;
        if binding.target != *target {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(binding)
    }

    pub async fn runtime_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<ManagedRuntimeSnapshot, AgentRunProductProjectionError> {
        let binding = self.binding(target).await?;
        let service = self
            .agents
            .resolve(&binding.agent.service_instance_id)
            .await
            .map_err(AgentRunProductProjectionError::Runtime)?;
        let snapshot = service
            .read(AgentReadQuery {
                source: binding.agent.source.clone(),
                at_revision: None,
            })
            .await
            .map_err(|error| AgentRunProductProjectionError::Runtime(error.to_string()))?;
        project_authoritative_agent_snapshot(binding.runtime_thread_id, snapshot)
            .map_err(|error| AgentRunProductProjectionError::Runtime(error.to_string()))
    }

    pub async fn runtime_snapshot_observation(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunProductRuntimeSnapshotObservation, AgentRunProductProjectionError> {
        let Some(binding) = self
            .runtime_bindings
            .load_product_binding(target)
            .await
            .map_err(AgentRunProductProjectionError::Binding)?
        else {
            return Ok(AgentRunProductRuntimeSnapshotObservation::Absent {
                requested_target: target.clone(),
            });
        };
        if binding.target != *target {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        let snapshot = self.runtime_snapshot(target).await?;
        Ok(AgentRunProductRuntimeSnapshotObservation::Current {
            product_binding: binding,
            snapshot,
        })
    }

    /// Reads the current Runtime thread strictly as optional presentation data.
    ///
    /// Product-to-thread identity is required to locate the thread. Source binding evidence is a
    /// command/recovery fence and is deliberately not part of this read boundary.
    pub async fn runtime_presentation_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<ManagedRuntimeSnapshot>, AgentRunProductProjectionError> {
        let Some(binding) = self
            .runtime_bindings
            .load_product_binding(target)
            .await
            .map_err(AgentRunProductProjectionError::Binding)?
        else {
            return Ok(None);
        };
        if binding.target != *target {
            return Ok(None);
        }
        match self.runtime_snapshot(target).await {
            Ok(snapshot) => Ok(Some(snapshot)),
            Err(AgentRunProductProjectionError::Runtime(_))
            | Err(AgentRunProductProjectionError::TargetNotBound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub async fn runtime_live_events(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Box<dyn AgentLiveEventStream>, AgentRunProductProjectionError> {
        let binding = self.binding(target).await?;
        let service = self
            .agents
            .resolve(&binding.agent.service_instance_id)
            .await
            .map_err(AgentRunProductProjectionError::Runtime)?;
        service
            .live_events(binding.agent.source)
            .await
            .map_err(|error| AgentRunProductProjectionError::Runtime(error.to_string()))
    }

    pub async fn workspace_presentation_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<WorkspaceModulePresentationSnapshot, AgentRunProductProjectionError> {
        let snapshot = self
            .workspace_presentations
            .load_snapshot(target)
            .await
            .map_err(|error| AgentRunProductProjectionError::Workspace(error.to_string()))?;
        if snapshot.target != *target {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(snapshot)
    }

    pub async fn workspace_presentation_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<WorkspaceModulePresentationChangeSequence>,
        limit: usize,
    ) -> Result<WorkspaceModulePresentationChangePage, AgentRunProductProjectionError> {
        let page = self
            .workspace_presentations
            .load_changes(target, after, limit)
            .await
            .map_err(|error| AgentRunProductProjectionError::Workspace(error.to_string()))?;
        if page.target != *target || page.changes.iter().any(|change| change.target != *target) {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(page)
    }

    pub async fn acknowledge_workspace_presentation(
        &self,
        request: WorkspaceModulePresentationAcknowledgeRequest,
    ) -> Result<WorkspaceModulePresentationChange, AgentRunProductProjectionError> {
        let target = request.target.clone();
        let change = self
            .workspace_presentation_acknowledgements
            .acknowledge(request)
            .await
            .map_err(|error| AgentRunProductProjectionError::Workspace(error.to_string()))?;
        if change.target != target || change.intent.target != target {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(change)
    }

    pub async fn terminal_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunTerminalSnapshot, AgentRunProductProjectionError> {
        let snapshot = self
            .terminals
            .load_snapshot(target)
            .await
            .map_err(|error| AgentRunProductProjectionError::Terminal(error.to_string()))?;
        if snapshot.target != *target
            || snapshot
                .terminals
                .iter()
                .any(|terminal| terminal.owner.target != *target)
        {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(snapshot)
    }

    pub async fn terminal_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<AgentRunTerminalChangeSequence>,
        limit: usize,
    ) -> Result<AgentRunTerminalChangePage, AgentRunProductProjectionError> {
        let page = self
            .terminals
            .load_changes(target, after, limit)
            .await
            .map_err(|error| AgentRunProductProjectionError::Terminal(error.to_string()))?;
        if page.target != *target
            || page
                .changes
                .iter()
                .any(|change| change.target != *target || change.delta.owner().target != *target)
        {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(page)
    }
}

#[async_trait]
pub trait AgentRunProductProjectionQueryPort: Send + Sync {
    async fn runtime_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<ManagedRuntimeSnapshot, AgentRunProductProjectionError>;
    async fn runtime_snapshot_observation(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunProductRuntimeSnapshotObservation, AgentRunProductProjectionError>;
    async fn runtime_presentation_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<ManagedRuntimeSnapshot>, AgentRunProductProjectionError> {
        Ok(match self.runtime_snapshot_observation(target).await? {
            AgentRunProductRuntimeSnapshotObservation::Absent { .. } => None,
            AgentRunProductRuntimeSnapshotObservation::Current { snapshot, .. } => Some(snapshot),
        })
    }
    async fn runtime_live_events(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Box<dyn AgentLiveEventStream>, AgentRunProductProjectionError>;
    async fn workspace_presentation_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<WorkspaceModulePresentationSnapshot, AgentRunProductProjectionError>;
    async fn workspace_presentation_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<WorkspaceModulePresentationChangeSequence>,
        limit: usize,
    ) -> Result<WorkspaceModulePresentationChangePage, AgentRunProductProjectionError>;
    async fn acknowledge_workspace_presentation(
        &self,
        request: WorkspaceModulePresentationAcknowledgeRequest,
    ) -> Result<WorkspaceModulePresentationChange, AgentRunProductProjectionError>;
    async fn terminal_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunTerminalSnapshot, AgentRunProductProjectionError>;
    async fn terminal_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<AgentRunTerminalChangeSequence>,
        limit: usize,
    ) -> Result<AgentRunTerminalChangePage, AgentRunProductProjectionError>;
}

#[async_trait]
impl AgentRunProductProjectionQueryPort for AgentRunProductProjectionGateway {
    async fn runtime_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<ManagedRuntimeSnapshot, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::runtime_snapshot(self, target).await
    }

    async fn runtime_snapshot_observation(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunProductRuntimeSnapshotObservation, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::runtime_snapshot_observation(self, target).await
    }

    async fn runtime_presentation_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<ManagedRuntimeSnapshot>, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::runtime_presentation_snapshot(self, target).await
    }

    async fn runtime_live_events(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Box<dyn AgentLiveEventStream>, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::runtime_live_events(self, target).await
    }

    async fn workspace_presentation_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<WorkspaceModulePresentationSnapshot, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::workspace_presentation_snapshot(self, target).await
    }

    async fn workspace_presentation_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<WorkspaceModulePresentationChangeSequence>,
        limit: usize,
    ) -> Result<WorkspaceModulePresentationChangePage, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::workspace_presentation_changes(self, target, after, limit)
            .await
    }

    async fn acknowledge_workspace_presentation(
        &self,
        request: WorkspaceModulePresentationAcknowledgeRequest,
    ) -> Result<WorkspaceModulePresentationChange, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::acknowledge_workspace_presentation(self, request).await
    }

    async fn terminal_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunTerminalSnapshot, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::terminal_snapshot(self, target).await
    }

    async fn terminal_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<AgentRunTerminalChangeSequence>,
        limit: usize,
    ) -> Result<AgentRunTerminalChangePage, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::terminal_changes(self, target, after, limit).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRunProductProjectionError {
    #[error("AgentRun Runtime binding load failed: {0}")]
    Binding(String),
    #[error("AgentRun target has no committed Runtime binding")]
    TargetNotBound,
    #[error("Managed Runtime projection load failed: {0}")]
    Runtime(String),
    #[error("Product projection returned a different AgentRun target")]
    TargetMismatch,
    #[error("Workspace Module presentation projection load failed: {0}")]
    Workspace(String),
    #[error("AgentRun terminal projection load failed: {0}")]
    Terminal(String),
}

#[cfg(test)]
mod product_runtime_binding_digest_tests {
    use agentdash_agent_runtime_contract::RuntimeThreadId;
    use agentdash_domain::agent_run_target::AgentRunTarget;
    use uuid::Uuid;

    use super::AgentRunProductRuntimeBinding;
    use crate::agent_run::{ProductAgentFrameRef, ProductExecutionProfileRef};

    #[test]
    fn binding_digest_ignores_recursive_json_object_order() {
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let mut left_profile = ProductExecutionProfileRef {
            profile_key: "codex".to_owned(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::json!({
                "z_option": true,
                "nested": {"z": 2, "a": 1},
                "a_option": false,
            }),
            credential_scope: None,
        };
        left_profile.refresh_digest();
        let mut right_profile = ProductExecutionProfileRef {
            configuration: serde_json::from_str(
                r#"{"a_option":false,"nested":{"a":1,"z":2},"z_option":true}"#,
            )
            .expect("equivalent configuration"),
            ..left_profile.clone()
        };
        right_profile.refresh_digest();
        let frame_id = Uuid::new_v4();
        let binding =
            |execution_profile: ProductExecutionProfileRef| AgentRunProductRuntimeBinding {
                target: target.clone(),
                runtime_thread_id: RuntimeThreadId::new("thread-canonical-digest")
                    .expect("runtime thread"),
                agent: crate::agent_run::AgentRunCompleteAgentAssociation {
                    service_instance_id: agentdash_agent_service_api::AgentServiceInstanceId::new(
                        "fixture-agent",
                    )
                    .unwrap(),
                    source: agentdash_agent_service_api::AgentSourceCoordinate::new(
                        "fixture-source",
                    )
                    .unwrap(),
                },
                launch_frame: ProductAgentFrameRef {
                    frame_id,
                    agent_id: target.agent_id,
                    revision: 1,
                },
                execution_profile_digest: execution_profile.profile_digest.clone(),
                execution_profile,
            };

        assert_eq!(
            binding(left_profile)
                .calculated_digest()
                .expect("left digest"),
            binding(right_profile)
                .calculated_digest()
                .expect("right digest")
        );
    }
}
