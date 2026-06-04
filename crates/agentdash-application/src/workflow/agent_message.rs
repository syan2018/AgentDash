use async_trait::async_trait;
use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, LifecycleAgent, LifecycleAgentRepository, LifecycleRun,
    LifecycleRunRepository, RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::AgentConfig;
use agentdash_spi::platform::auth::AuthIdentity;

use crate::session::{LaunchCommand, SessionLaunchService, UserPromptInput};
use crate::workflow::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct LifecycleAgentMessageCommand {
    pub delivery_runtime_session_id: String,
    pub input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub executor_config: Option<AgentConfig>,
    pub identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleAgentMessageDispatch {
    pub runtime_session_id: String,
    pub turn_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
}

#[derive(Debug, Clone)]
pub struct LifecycleAgentMessageDelivery {
    pub delivery_runtime_session_id: String,
    pub input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub executor_config: Option<AgentConfig>,
    pub identity: Option<AuthIdentity>,
}

#[async_trait]
pub trait LifecycleAgentMessageDeliveryPort: Send + Sync {
    async fn deliver_user_message(
        &self,
        delivery: LifecycleAgentMessageDelivery,
    ) -> Result<String, WorkflowApplicationError>;
}

#[derive(Clone)]
pub struct SessionLaunchLifecycleAgentMessageDeliveryPort {
    session_launch: SessionLaunchService,
}

impl SessionLaunchLifecycleAgentMessageDeliveryPort {
    pub fn new(session_launch: SessionLaunchService) -> Self {
        Self { session_launch }
    }
}

#[async_trait]
impl LifecycleAgentMessageDeliveryPort for SessionLaunchLifecycleAgentMessageDeliveryPort {
    async fn deliver_user_message(
        &self,
        delivery: LifecycleAgentMessageDelivery,
    ) -> Result<String, WorkflowApplicationError> {
        let user_input = UserPromptInput {
            input: Some(delivery.input),
            env: Default::default(),
            executor_config: delivery.executor_config,
            backend_selection: None,
        };
        user_input
            .resolve_prompt_payload()
            .map_err(WorkflowApplicationError::BadRequest)?;
        let command =
            LaunchCommand::lifecycle_agent_user_message_input(user_input, delivery.identity);
        self.session_launch
            .launch_command(&delivery.delivery_runtime_session_id, command)
            .await
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!(
                    "LifecycleAgent 用户消息投递失败: {error}"
                ))
            })
    }
}

pub struct LifecycleAgentMessageService<'a, D> {
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    agent_frame_repo: &'a dyn AgentFrameRepository,
    execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    delivery: D,
}

impl<'a, D> LifecycleAgentMessageService<'a, D>
where
    D: LifecycleAgentMessageDeliveryPort,
{
    pub fn new(
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
        agent_frame_repo: &'a dyn AgentFrameRepository,
        execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
        delivery: D,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_agent_repo,
            agent_frame_repo,
            execution_anchor_repo,
            delivery,
        }
    }

    pub async fn dispatch_user_message(
        &self,
        command: LifecycleAgentMessageCommand,
    ) -> Result<LifecycleAgentMessageDispatch, WorkflowApplicationError> {
        if command.delivery_runtime_session_id.trim().is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "delivery runtime session id 不能为空".to_string(),
            ));
        }
        if command.input.is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "input 不能为空".to_string(),
            ));
        }

        let (run, agent, frame) = self
            .resolve_control_plane(&command.delivery_runtime_session_id)
            .await?;

        let turn_id = self
            .delivery
            .deliver_user_message(LifecycleAgentMessageDelivery {
                delivery_runtime_session_id: command.delivery_runtime_session_id.clone(),
                input: command.input,
                executor_config: command.executor_config,
                identity: command.identity,
            })
            .await?;

        Ok(LifecycleAgentMessageDispatch {
            runtime_session_id: command.delivery_runtime_session_id,
            turn_id,
            run_id: run.id,
            agent_id: agent.id,
            frame_id: frame.id,
            frame_revision: frame.revision,
        })
    }

    async fn resolve_control_plane(
        &self,
        runtime_session_id: &str,
    ) -> Result<(LifecycleRun, LifecycleAgent, AgentFrame), WorkflowApplicationError> {
        let anchor = self
            .execution_anchor_repo
            .find_by_session(runtime_session_id)
            .await?;

        let anchor = anchor.ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!(
                "runtime_session 缺少 RuntimeSessionExecutionAnchor: {runtime_session_id}"
            ))
        })?;
        let agent = self
            .lifecycle_agent_repo
            .get(anchor.agent_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle_agent 不存在: {}",
                    anchor.agent_id
                ))
            })?;
        if agent.run_id != anchor.run_id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "RuntimeSessionExecutionAnchor run {} 与 LifecycleAgent run {} 不一致",
                anchor.run_id, agent.run_id
            )));
        }
        let run = self
            .lifecycle_run_repo
            .get_by_id(anchor.run_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle_run 不存在: {}",
                    anchor.run_id
                ))
            })?;
        let frame = self
            .agent_frame_repo
            .get_current(agent.id)
            .await?
            .or(self.agent_frame_repo.get(anchor.launch_frame_id).await?)
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle_agent {} 没有 current AgentFrame",
                    agent.id
                ))
            })?;
        self.validate_frame(runtime_session_id, &agent, &frame)?;
        Ok((run, agent, frame))
    }

    fn validate_frame(
        &self,
        _runtime_session_id: &str,
        agent: &LifecycleAgent,
        frame: &AgentFrame,
    ) -> Result<(), WorkflowApplicationError> {
        if frame.agent_id != agent.id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "AgentFrame {} 不属于 LifecycleAgent {}",
                frame.id, agent.id
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        AgentFrameRepository, LifecycleAgentRepository, LifecycleRunRepository,
        RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
    };
    use chrono::Utc;
    use std::sync::Mutex;

    #[derive(Default)]
    struct InMemoryRunRepo {
        items: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait]
    impl LifecycleRunRepository for InMemoryRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|run| ids.contains(&run.id))
                .cloned()
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_by_root_graph(
            &self,
            root_graph_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.root_graph_id == Some(root_graph_id))
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|existing| existing.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items.lock().unwrap().retain(|run| run.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryAgentRepo {
        items: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait]
    impl LifecycleAgentRepository for InMemoryAgentRepo {
        async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(agent.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|agent| agent.id == id)
                .cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|agent| agent.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|existing| existing.id == agent.id) {
                *existing = agent.clone();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryFrameRepo {
        items: Mutex<Vec<AgentFrame>>,
    }

    #[async_trait]
    impl AgentFrameRepository for InMemoryFrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(frame.clone());
            Ok(())
        }

        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned())
        }

        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            let mut frames = self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect::<Vec<_>>();
            frames.sort_by_key(|frame| frame.revision);
            Ok(frames.pop())
        }

        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn append_visible_canvas_mount(
            &self,
            _frame_id: Uuid,
            _mount_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryAnchorRepo {
        items: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for InMemoryAnchorRepo {
        async fn upsert(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(anchor.clone());
            Ok(())
        }

        async fn update_assignment(
            &self,
            _runtime_session_id: &str,
            _assignment_id: Uuid,
            _attempt: i32,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.items
                .lock()
                .unwrap()
                .retain(|anchor| anchor.runtime_session_id != runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|anchor| anchor.runtime_session_id == runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| runtime_session_ids.contains(&anchor.runtime_session_id))
                .cloned()
                .collect())
        }

        async fn latest_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.agent_id == agent_id)
                .max_by_key(|anchor| anchor.updated_at)
                .cloned())
        }
    }

    #[derive(Default)]
    struct FakeDelivery {
        calls: Mutex<Vec<LifecycleAgentMessageDelivery>>,
    }

    #[async_trait]
    impl LifecycleAgentMessageDeliveryPort for &FakeDelivery {
        async fn deliver_user_message(
            &self,
            delivery: LifecycleAgentMessageDelivery,
        ) -> Result<String, WorkflowApplicationError> {
            self.calls.lock().unwrap().push(delivery);
            Ok("turn-1".to_string())
        }
    }

    fn seed_control_plane(
        run_repo: &InMemoryRunRepo,
        agent_repo: &InMemoryAgentRepo,
        frame_repo: &InMemoryFrameRepo,
        anchor_repo: &InMemoryAnchorRepo,
        runtime_session_id: &str,
    ) -> (LifecycleRun, LifecycleAgent, AgentFrame) {
        let project_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id, Uuid::new_v4());
        run.created_at = Utc::now();
        let mut agent = LifecycleAgent::new_root(run.id, project_id, "project_agent");
        let frame = AgentFrame::new_revision(agent.id, 1, "test");
        agent.set_current_frame(frame.id);
        run_repo.items.lock().unwrap().push(run.clone());
        agent_repo.items.lock().unwrap().push(agent.clone());
        frame_repo.items.lock().unwrap().push(frame.clone());
        anchor_repo
            .items
            .lock()
            .unwrap()
            .push(RuntimeSessionExecutionAnchor::new_dispatch(
                runtime_session_id,
                run.id,
                frame.id,
                agent.id,
                None,
                None,
            ));
        (run, agent, frame)
    }

    #[tokio::test]
    async fn dispatch_user_message_resolves_anchor_and_delegates_delivery() {
        let run_repo = InMemoryRunRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let anchor_repo = InMemoryAnchorRepo::default();
        let delivery = FakeDelivery::default();
        let (run, agent, frame) = seed_control_plane(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            "runtime-1",
        );
        let service = LifecycleAgentMessageService::new(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            &delivery,
        );

        let result = service
            .dispatch_user_message(LifecycleAgentMessageCommand {
                delivery_runtime_session_id: "runtime-1".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                executor_config: None,
                identity: None,
            })
            .await
            .expect("dispatch");

        assert_eq!(result.run_id, run.id);
        assert_eq!(result.agent_id, agent.id);
        assert_eq!(result.frame_id, frame.id);
        assert_eq!(result.runtime_session_id, "runtime-1");
        let calls = delivery.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].delivery_runtime_session_id, "runtime-1");
    }

    #[tokio::test]
    async fn dispatch_user_message_rejects_unresolved_runtime_session() {
        let run_repo = InMemoryRunRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let anchor_repo = InMemoryAnchorRepo::default();
        let delivery = FakeDelivery::default();
        let service = LifecycleAgentMessageService::new(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            &delivery,
        );

        let error = service
            .dispatch_user_message(LifecycleAgentMessageCommand {
                delivery_runtime_session_id: "missing".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                executor_config: None,
                identity: None,
            })
            .await
            .expect_err("missing runtime session should fail");

        assert!(matches!(error, WorkflowApplicationError::NotFound(_)));
        assert!(delivery.calls.lock().unwrap().is_empty());
    }
}
