use std::sync::Arc;

use agentdash_agent_runtime::project_authoritative_agent_view;
use agentdash_agent_runtime_contract::{AgentRuntimeUpdate, AgentRuntimeView, RuntimeThreadId};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentContextQuery, AgentContextSnapshot, AgentLiveEventStream,
    AgentReadQuery, AgentServiceError, AgentSourceCoordinate, CompleteAgentService,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::AgentRunCompleteAgentAssociation;
use super::terminal_projection_protocol::{
    AgentRunTerminalChangePage, AgentRunTerminalChangeSequence,
    AgentRunTerminalProjectionRepository, AgentRunTerminalSnapshot,
};
use super::{ProductAgentFrameRef, ProductExecutionProfileRef};

pub struct AgentRunResolvedCompleteAgent {
    pub service: Arc<dyn CompleteAgentService>,
    pub binding_generation: AgentBindingGeneration,
}

#[async_trait]
pub trait AgentRuntimeUpdateStream: Send {
    async fn next(&mut self) -> Result<Option<AgentRuntimeUpdate>, AgentRunProductProjectionError>;
}

struct CompleteAgentRuntimeUpdateStream {
    service: Arc<dyn CompleteAgentService>,
    source: AgentSourceCoordinate,
    runtime_thread_id: RuntimeThreadId,
    live: Box<dyn AgentLiveEventStream>,
}

#[async_trait]
impl AgentRuntimeUpdateStream for CompleteAgentRuntimeUpdateStream {
    async fn next(&mut self) -> Result<Option<AgentRuntimeUpdate>, AgentRunProductProjectionError> {
        let Some(event) = self
            .live
            .next()
            .await
            .map_err(AgentRunProductProjectionError::Agent)?
        else {
            return Ok(None);
        };
        if event.source != self.source {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        let snapshot = self
            .service
            .read(AgentReadQuery {
                source: self.source.clone(),
                at_revision: None,
            })
            .await
            .map_err(AgentRunProductProjectionError::Agent)?;
        let view = project_authoritative_agent_view(self.runtime_thread_id.clone(), snapshot)
            .map_err(|error| AgentRunProductProjectionError::Runtime(error.to_string()))?;
        Ok(Some(AgentRuntimeUpdate {
            lane_sequence: event.sequence.0,
            view_revision: view.view_revision,
            execution: view.execution,
            command_availability: view.command_availability,
            interactions: view.interactions,
            presentations: vec![event.record],
        }))
    }
}

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
pub enum AgentRunProductRuntimeViewObservation {
    Absent {
        requested_target: AgentRunTarget,
    },
    Current {
        product_binding: AgentRunProductRuntimeBinding,
        view: AgentRuntimeView,
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
    terminals: Arc<dyn AgentRunTerminalProjectionRepository>,
}

#[async_trait]
pub trait AgentRunCompleteAgentResolverPort: Send + Sync {
    async fn resolve(
        &self,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentRunResolvedCompleteAgent, String>;
}

impl AgentRunProductProjectionGateway {
    pub fn new(
        runtime_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        agents: Arc<dyn AgentRunCompleteAgentResolverPort>,
        terminals: Arc<dyn AgentRunTerminalProjectionRepository>,
    ) -> Self {
        Self {
            runtime_bindings,
            agents,
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

    pub async fn runtime_view(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRuntimeView, AgentRunProductProjectionError> {
        let binding = self.binding(target).await?;
        let resolved = self
            .agents
            .resolve(&binding)
            .await
            .map_err(AgentRunProductProjectionError::Runtime)?;
        let snapshot = resolved
            .service
            .read(AgentReadQuery {
                source: binding.agent.source.clone(),
                at_revision: None,
            })
            .await
            .map_err(AgentRunProductProjectionError::Agent)?;
        project_authoritative_agent_view(binding.runtime_thread_id, snapshot)
            .map_err(|error| AgentRunProductProjectionError::Runtime(error.to_string()))
    }

    pub async fn runtime_product_binding(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, AgentRunProductProjectionError> {
        let binding = self
            .runtime_bindings
            .load_product_binding(target)
            .await
            .map_err(AgentRunProductProjectionError::Binding)?;
        if binding
            .as_ref()
            .is_some_and(|binding| binding.target != *target)
        {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(binding)
    }

    pub async fn context_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentContextSnapshot, AgentRunProductProjectionError> {
        let binding = self.binding(target).await?;
        let resolved = self
            .agents
            .resolve(&binding)
            .await
            .map_err(AgentRunProductProjectionError::Runtime)?;
        resolved
            .service
            .context(AgentContextQuery {
                source: binding.agent.source,
                at_revision: None,
            })
            .await
            .map_err(AgentRunProductProjectionError::Agent)
    }

    pub async fn runtime_view_observation(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunProductRuntimeViewObservation, AgentRunProductProjectionError> {
        let Some(binding) = self
            .runtime_bindings
            .load_product_binding(target)
            .await
            .map_err(AgentRunProductProjectionError::Binding)?
        else {
            return Ok(AgentRunProductRuntimeViewObservation::Absent {
                requested_target: target.clone(),
            });
        };
        if binding.target != *target {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        let view = self.runtime_view(target).await?;
        Ok(AgentRunProductRuntimeViewObservation::Current {
            product_binding: binding,
            view,
        })
    }

    pub async fn runtime_updates(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Box<dyn AgentRuntimeUpdateStream>, AgentRunProductProjectionError> {
        let binding = self.binding(target).await?;
        let resolved = self
            .agents
            .resolve(&binding)
            .await
            .map_err(AgentRunProductProjectionError::Runtime)?;
        let live = resolved
            .service
            .live_events(binding.agent.source.clone())
            .await
            .map_err(AgentRunProductProjectionError::Agent)?;
        Ok(Box::new(CompleteAgentRuntimeUpdateStream {
            service: resolved.service,
            source: binding.agent.source,
            runtime_thread_id: binding.runtime_thread_id,
            live,
        }))
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
    async fn runtime_product_binding(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, AgentRunProductProjectionError> {
        Ok(match self.runtime_view_observation(target).await? {
            AgentRunProductRuntimeViewObservation::Absent { .. } => None,
            AgentRunProductRuntimeViewObservation::Current {
                product_binding, ..
            } => Some(product_binding),
        })
    }
    async fn runtime_view(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRuntimeView, AgentRunProductProjectionError>;
    async fn context_snapshot(
        &self,
        _target: &AgentRunTarget,
    ) -> Result<AgentContextSnapshot, AgentRunProductProjectionError> {
        Err(AgentRunProductProjectionError::Runtime(
            "Product projection does not expose Agent context".to_owned(),
        ))
    }
    async fn runtime_view_observation(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunProductRuntimeViewObservation, AgentRunProductProjectionError>;
    async fn runtime_updates(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Box<dyn AgentRuntimeUpdateStream>, AgentRunProductProjectionError>;
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
    async fn runtime_product_binding(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::runtime_product_binding(self, target).await
    }

    async fn runtime_view(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRuntimeView, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::runtime_view(self, target).await
    }

    async fn context_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentContextSnapshot, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::context_snapshot(self, target).await
    }

    async fn runtime_view_observation(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunProductRuntimeViewObservation, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::runtime_view_observation(self, target).await
    }

    async fn runtime_updates(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Box<dyn AgentRuntimeUpdateStream>, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::runtime_updates(self, target).await
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
    #[error("Agent Runtime projection load failed: {0}")]
    Runtime(String),
    #[error("Agent Runtime Agent service failed: {0}")]
    Agent(AgentServiceError),
    #[error("Product projection returned a different AgentRun target")]
    TargetMismatch,
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
