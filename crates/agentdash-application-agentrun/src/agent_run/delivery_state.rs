use agentdash_agent_protocol::{ControlPlaneProjectionChangeReason, RuntimeTerminalDiagnostic};
use agentdash_application_ports::project_projection_notification::{
    ProjectProjectionInvalidation, ProjectProjectionNotificationPort,
};
use agentdash_domain::workflow::{
    AgentRunDeliveryBinding, AgentRunDeliveryBindingRepository, DeliveryBindingStatus,
    LifecycleAgentRepository, RuntimeSessionExecutionAnchorRepository,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::WorkflowApplicationError;

pub struct AgentRunDeliveryStateRepos<'a> {
    pub execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    pub delivery_binding_repo: &'a dyn AgentRunDeliveryBindingRepository,
    pub lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    pub project_projection_notifications: Option<&'a dyn ProjectProjectionNotificationPort>,
}

pub struct AgentRunDeliveryStateService<'a> {
    repos: AgentRunDeliveryStateRepos<'a>,
}

impl<'a> AgentRunDeliveryStateService<'a> {
    pub fn new(repos: AgentRunDeliveryStateRepos<'a>) -> Self {
        Self { repos }
    }

    pub async fn mark_running_from_accepted_turn(
        &self,
        input: AgentRunRunningTransitionInput,
    ) -> Result<Option<AgentRunDeliveryTransition>, WorkflowApplicationError> {
        let Some(anchor) = self
            .repos
            .execution_anchor_repo
            .find_by_session(&input.runtime_session_id)
            .await?
        else {
            return Ok(None);
        };

        let binding = match self
            .repos
            .delivery_binding_repo
            .get_current(anchor.run_id, anchor.agent_id)
            .await?
        {
            Some(binding) if binding.runtime_session_id == input.runtime_session_id => binding,
            Some(_) => return Ok(None),
            None => AgentRunDeliveryBinding::from_anchor(
                &anchor,
                DeliveryBindingStatus::Running,
                input.observed_at,
            ),
        }
        .mark_running(input.turn_id.clone(), input.observed_at);
        let persisted = self
            .repos
            .delivery_binding_repo
            .upsert_if_current_runtime_session(&binding)
            .await?;
        if !persisted {
            return Ok(None);
        }

        let transition = AgentRunDeliveryTransition {
            run_id: anchor.run_id,
            agent_id: anchor.agent_id,
            frame_id: Some(input.frame_id),
            delivery_runtime_session_id: input.runtime_session_id,
        };
        self.publish_list_invalidation(
            transition.run_id,
            transition.agent_id,
            transition.frame_id,
            ControlPlaneProjectionChangeReason::AgentRunActivityChanged,
            Some(transition.delivery_runtime_session_id.clone()),
        )
        .await;

        Ok(Some(transition))
    }

    pub async fn mark_terminal_from_runtime_session(
        &self,
        input: AgentRunTerminalTransitionInput,
    ) -> Result<Option<AgentRunDeliveryTransition>, WorkflowApplicationError> {
        let Some(anchor) = self
            .repos
            .execution_anchor_repo
            .find_by_session(&input.runtime_session_id)
            .await?
        else {
            return Ok(None);
        };

        let binding = match self
            .repos
            .delivery_binding_repo
            .get_current(anchor.run_id, anchor.agent_id)
            .await?
        {
            Some(binding) if binding.runtime_session_id == input.runtime_session_id => binding,
            Some(_) => return Ok(None),
            None => AgentRunDeliveryBinding::from_anchor(
                &anchor,
                DeliveryBindingStatus::Terminal,
                input.observed_at,
            ),
        }
        .mark_terminal(
            input.turn_id.clone(),
            input.terminal_state.clone(),
            input.terminal_message.clone(),
            input
                .terminal_diagnostic
                .as_ref()
                .and_then(|diagnostic| serde_json::to_value(diagnostic).ok()),
            input.observed_at,
        );
        let persisted = self
            .repos
            .delivery_binding_repo
            .upsert_if_current_runtime_session(&binding)
            .await?;
        if !persisted {
            return Ok(None);
        }

        let transition = AgentRunDeliveryTransition {
            run_id: anchor.run_id,
            agent_id: anchor.agent_id,
            frame_id: input.frame_id,
            delivery_runtime_session_id: input.runtime_session_id,
        };
        self.publish_list_invalidation(
            transition.run_id,
            transition.agent_id,
            transition.frame_id,
            ControlPlaneProjectionChangeReason::DeliveryTerminal,
            Some(transition.delivery_runtime_session_id.clone()),
        )
        .await;

        Ok(Some(transition))
    }

    pub async fn publish_terminal_binding_invalidation(
        &self,
        binding: &AgentRunDeliveryBinding,
        frame_id: Option<Uuid>,
    ) -> AgentRunDeliveryTransition {
        let transition = AgentRunDeliveryTransition {
            run_id: binding.run_id,
            agent_id: binding.agent_id,
            frame_id,
            delivery_runtime_session_id: binding.runtime_session_id.clone(),
        };
        self.publish_list_invalidation(
            transition.run_id,
            transition.agent_id,
            transition.frame_id,
            ControlPlaneProjectionChangeReason::DeliveryTerminal,
            Some(transition.delivery_runtime_session_id.clone()),
        )
        .await;
        transition
    }

    async fn publish_list_invalidation(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        frame_id: Option<Uuid>,
        reason: ControlPlaneProjectionChangeReason,
        delivery_runtime_session_id: Option<String>,
    ) {
        let Some(port) = self.repos.project_projection_notifications else {
            return;
        };
        let Ok(Some(agent)) = self.repos.lifecycle_agent_repo.get(agent_id).await else {
            return;
        };
        let _ = port
            .publish_project_projection_invalidated(ProjectProjectionInvalidation::agent_run_list(
                agent.project_id,
                run_id,
                agent_id,
                frame_id,
                reason,
                delivery_runtime_session_id,
            ))
            .await;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunDeliveryTransition {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Option<Uuid>,
    pub delivery_runtime_session_id: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunRunningTransitionInput {
    pub runtime_session_id: String,
    pub turn_id: String,
    pub frame_id: Uuid,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AgentRunTerminalTransitionInput {
    pub runtime_session_id: String,
    pub turn_id: String,
    pub frame_id: Option<Uuid>,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
    pub terminal_diagnostic: Option<RuntimeTerminalDiagnostic>,
    pub observed_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::workflow_repositories::{
        MemoryAgentRunDeliveryBindingRepository, MemoryLifecycleAgentRepository,
        MemoryRuntimeSessionExecutionAnchorRepository,
    };
    use agentdash_application_ports::project_projection_notification::{
        ProjectProjectionInvalidation, ProjectProjectionNotificationPort,
    };
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        AgentRunDeliveryBindingRepository, DeliveryBindingStatus, LifecycleAgent,
        RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
    };
    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    #[derive(Default)]
    struct RecordingProjectProjectionNotificationPort {
        items: Mutex<Vec<ProjectProjectionInvalidation>>,
    }

    #[async_trait]
    impl ProjectProjectionNotificationPort for RecordingProjectProjectionNotificationPort {
        async fn publish_project_projection_invalidated(
            &self,
            invalidation: ProjectProjectionInvalidation,
        ) -> Result<(), String> {
            self.items.lock().await.push(invalidation);
            Ok(())
        }
    }

    impl RecordingProjectProjectionNotificationPort {
        async fn recorded(&self) -> Vec<ProjectProjectionInvalidation> {
            self.items.lock().await.clone()
        }
    }

    struct RacingDeliveryBindingRepository {
        current: Mutex<Option<AgentRunDeliveryBinding>>,
        replacement_before_write: AgentRunDeliveryBinding,
    }

    #[async_trait]
    impl AgentRunDeliveryBindingRepository for RacingDeliveryBindingRepository {
        async fn upsert(&self, binding: &AgentRunDeliveryBinding) -> Result<(), DomainError> {
            *self.current.lock().await = Some(binding.clone());
            Ok(())
        }

        async fn upsert_if_current_runtime_session(
            &self,
            _binding: &AgentRunDeliveryBinding,
        ) -> Result<bool, DomainError> {
            *self.current.lock().await = Some(self.replacement_before_write.clone());
            Ok(false)
        }

        async fn get_current(
            &self,
            run_id: Uuid,
            agent_id: Uuid,
        ) -> Result<Option<AgentRunDeliveryBinding>, DomainError> {
            Ok(self
                .current
                .lock()
                .await
                .as_ref()
                .filter(|binding| binding.run_id == run_id && binding.agent_id == agent_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<AgentRunDeliveryBinding>, DomainError> {
            Ok(self
                .current
                .lock()
                .await
                .iter()
                .filter(|binding| binding.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            let mut current = self.current.lock().await;
            if current
                .as_ref()
                .is_some_and(|binding| binding.runtime_session_id == runtime_session_id)
            {
                *current = None;
            }
            Ok(())
        }
    }

    async fn create_agent(
        agents: &MemoryLifecycleAgentRepository,
        run_id: Uuid,
        agent_id: Uuid,
        project_id: Uuid,
    ) {
        let mut agent = LifecycleAgent::new_root(
            run_id,
            project_id,
            agentdash_domain::workflow::AgentSource::ProjectAgent,
        );
        agent.id = agent_id;
        agents.create(&agent).await.expect("agent");
    }

    #[tokio::test]
    async fn running_transition_updates_binding_and_invalidates_agent_run_list() {
        let anchors = MemoryRuntimeSessionExecutionAnchorRepository::default();
        let bindings = MemoryAgentRunDeliveryBindingRepository::default();
        let agents = MemoryLifecycleAgentRepository::default();
        let invalidations = RecordingProjectProjectionNotificationPort::default();
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame_id = Uuid::new_v4();
        let anchor =
            RuntimeSessionExecutionAnchor::new_dispatch("runtime-a", run_id, frame_id, agent_id);
        anchors.create_once(&anchor).await.expect("anchor");
        create_agent(&agents, run_id, agent_id, project_id).await;
        bindings
            .upsert(&AgentRunDeliveryBinding::from_anchor(
                &anchor,
                DeliveryBindingStatus::Ready,
                Utc::now(),
            ))
            .await
            .expect("ready binding");

        let service = AgentRunDeliveryStateService::new(AgentRunDeliveryStateRepos {
            execution_anchor_repo: &anchors,
            delivery_binding_repo: &bindings,
            lifecycle_agent_repo: &agents,
            project_projection_notifications: Some(&invalidations),
        });
        let transition = service
            .mark_running_from_accepted_turn(AgentRunRunningTransitionInput {
                runtime_session_id: "runtime-a".to_string(),
                turn_id: "turn-1".to_string(),
                frame_id,
                observed_at: Utc::now(),
            })
            .await
            .expect("running transition")
            .expect("transition");

        assert_eq!(transition.run_id, run_id);
        assert_eq!(transition.agent_id, agent_id);
        assert_eq!(transition.frame_id, Some(frame_id));
        assert_eq!(transition.delivery_runtime_session_id, "runtime-a");
        let binding = bindings
            .get_current(run_id, agent_id)
            .await
            .expect("binding read")
            .expect("binding");
        assert_eq!(binding.status, DeliveryBindingStatus::Running);
        assert_eq!(binding.active_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(binding.last_turn_id.as_deref(), Some("turn-1"));
        let recorded = invalidations.recorded().await;
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].project_id, project_id);
        assert_eq!(recorded[0].run_id, run_id);
        assert_eq!(recorded[0].agent_id, agent_id);
        assert_eq!(recorded[0].frame_id, Some(frame_id));
        assert_eq!(
            recorded[0].reason,
            ControlPlaneProjectionChangeReason::AgentRunActivityChanged
        );
        assert_eq!(
            recorded[0].delivery_runtime_session_id.as_deref(),
            Some("runtime-a")
        );
    }

    #[tokio::test]
    async fn terminal_transition_updates_agent_run_binding_and_invalidates_agent_run_list() {
        let anchors = MemoryRuntimeSessionExecutionAnchorRepository::default();
        let bindings = MemoryAgentRunDeliveryBindingRepository::default();
        let agents = MemoryLifecycleAgentRepository::default();
        let invalidations = RecordingProjectProjectionNotificationPort::default();
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame_id = Uuid::new_v4();
        let anchor =
            RuntimeSessionExecutionAnchor::new_dispatch("runtime-a", run_id, frame_id, agent_id);
        anchors.create_once(&anchor).await.expect("anchor");
        create_agent(&agents, run_id, agent_id, project_id).await;

        let service = AgentRunDeliveryStateService::new(AgentRunDeliveryStateRepos {
            execution_anchor_repo: &anchors,
            delivery_binding_repo: &bindings,
            lifecycle_agent_repo: &agents,
            project_projection_notifications: Some(&invalidations),
        });
        let transition = service
            .mark_terminal_from_runtime_session(AgentRunTerminalTransitionInput {
                runtime_session_id: "runtime-a".to_string(),
                turn_id: "turn-1".to_string(),
                frame_id: Some(frame_id),
                terminal_state: "failed".to_string(),
                terminal_message: Some("provider failed".to_string()),
                terminal_diagnostic: Some(RuntimeTerminalDiagnostic {
                    kind: "provider".to_string(),
                    code: Some("invalid_request".to_string()),
                    http_status: Some(400),
                    provider: Some("Example LLM".to_string()),
                    model: Some("example-chat-large".to_string()),
                    message: "provider failed".to_string(),
                    retryable: false,
                }),
                observed_at: Utc::now(),
            })
            .await
            .expect("terminal transition")
            .expect("transition");

        assert_eq!(transition.run_id, run_id);
        assert_eq!(transition.agent_id, agent_id);
        assert_eq!(transition.frame_id, Some(frame_id));
        assert_eq!(transition.delivery_runtime_session_id, "runtime-a");
        let binding = bindings
            .get_current(anchor.run_id, anchor.agent_id)
            .await
            .expect("binding read")
            .expect("binding");
        assert_eq!(binding.status, DeliveryBindingStatus::Terminal);
        assert_eq!(binding.active_turn_id, None);
        assert_eq!(binding.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(binding.terminal_state.as_deref(), Some("failed"));
        assert_eq!(binding.terminal_message.as_deref(), Some("provider failed"));
        assert_eq!(
            binding
                .terminal_diagnostic
                .as_ref()
                .and_then(|value| value.get("code"))
                .and_then(serde_json::Value::as_str),
            Some("invalid_request")
        );
        let recorded = invalidations.recorded().await;
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].project_id, project_id);
        assert_eq!(recorded[0].run_id, run_id);
        assert_eq!(recorded[0].agent_id, agent_id);
        assert_eq!(recorded[0].frame_id, Some(frame_id));
        assert_eq!(
            recorded[0].reason,
            ControlPlaneProjectionChangeReason::DeliveryTerminal
        );
        assert_eq!(
            recorded[0].delivery_runtime_session_id.as_deref(),
            Some("runtime-a")
        );
    }

    #[tokio::test]
    async fn terminal_transition_ignores_stale_runtime_without_invalidation() {
        let anchors = MemoryRuntimeSessionExecutionAnchorRepository::default();
        let bindings = MemoryAgentRunDeliveryBindingRepository::default();
        let agents = MemoryLifecycleAgentRepository::default();
        let invalidations = RecordingProjectProjectionNotificationPort::default();
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let launch_frame_id = Uuid::new_v4();
        let old_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-old",
            run_id,
            launch_frame_id,
            agent_id,
        );
        let current_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-current",
            run_id,
            launch_frame_id,
            agent_id,
        );
        anchors.create_once(&old_anchor).await.expect("old anchor");
        anchors
            .create_once(&current_anchor)
            .await
            .expect("current anchor");
        create_agent(&agents, run_id, agent_id, project_id).await;
        let current_binding = AgentRunDeliveryBinding::from_anchor(
            &current_anchor,
            DeliveryBindingStatus::Running,
            Utc::now(),
        )
        .mark_running("turn-current", Utc::now());
        bindings
            .upsert(&current_binding)
            .await
            .expect("current binding");

        let service = AgentRunDeliveryStateService::new(AgentRunDeliveryStateRepos {
            execution_anchor_repo: &anchors,
            delivery_binding_repo: &bindings,
            lifecycle_agent_repo: &agents,
            project_projection_notifications: Some(&invalidations),
        });
        let transition = service
            .mark_terminal_from_runtime_session(AgentRunTerminalTransitionInput {
                runtime_session_id: "runtime-old".to_string(),
                turn_id: "turn-old".to_string(),
                frame_id: Some(launch_frame_id),
                terminal_state: "failed".to_string(),
                terminal_message: Some("old runtime failed late".to_string()),
                terminal_diagnostic: None,
                observed_at: Utc::now(),
            })
            .await
            .expect("stale terminal transition");

        assert_eq!(transition, None);
        let binding = bindings
            .get_current(run_id, agent_id)
            .await
            .expect("binding read")
            .expect("binding");
        assert_eq!(binding.runtime_session_id, "runtime-current");
        assert_eq!(binding.status, DeliveryBindingStatus::Running);
        assert_eq!(binding.active_turn_id.as_deref(), Some("turn-current"));
        assert_eq!(binding.terminal_state, None);
        assert!(invalidations.recorded().await.is_empty());
    }

    #[tokio::test]
    async fn running_transition_lost_race_does_not_publish_or_overwrite_current_binding() {
        let anchors = MemoryRuntimeSessionExecutionAnchorRepository::default();
        let agents = MemoryLifecycleAgentRepository::default();
        let invalidations = RecordingProjectProjectionNotificationPort::default();
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let old_frame_id = Uuid::new_v4();
        let current_frame_id = Uuid::new_v4();
        let old_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-old",
            run_id,
            old_frame_id,
            agent_id,
        );
        let current_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-current",
            run_id,
            current_frame_id,
            agent_id,
        );
        anchors.create_once(&old_anchor).await.expect("old anchor");
        anchors
            .create_once(&current_anchor)
            .await
            .expect("current anchor");
        create_agent(&agents, run_id, agent_id, project_id).await;
        let old_binding = AgentRunDeliveryBinding::from_anchor(
            &old_anchor,
            DeliveryBindingStatus::Ready,
            Utc::now(),
        );
        let current_binding = AgentRunDeliveryBinding::from_anchor(
            &current_anchor,
            DeliveryBindingStatus::Running,
            Utc::now(),
        )
        .mark_running("turn-current", Utc::now());
        let bindings = RacingDeliveryBindingRepository {
            current: Mutex::new(Some(old_binding)),
            replacement_before_write: current_binding,
        };

        let service = AgentRunDeliveryStateService::new(AgentRunDeliveryStateRepos {
            execution_anchor_repo: &anchors,
            delivery_binding_repo: &bindings,
            lifecycle_agent_repo: &agents,
            project_projection_notifications: Some(&invalidations),
        });
        let transition = service
            .mark_running_from_accepted_turn(AgentRunRunningTransitionInput {
                runtime_session_id: "runtime-old".to_string(),
                turn_id: "turn-old".to_string(),
                frame_id: old_frame_id,
                observed_at: Utc::now(),
            })
            .await
            .expect("running transition");

        assert_eq!(transition, None);
        let binding = bindings
            .get_current(run_id, agent_id)
            .await
            .expect("binding read")
            .expect("binding");
        assert_eq!(binding.runtime_session_id, "runtime-current");
        assert_eq!(binding.active_turn_id.as_deref(), Some("turn-current"));
        assert!(invalidations.recorded().await.is_empty());
    }
}
