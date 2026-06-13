use async_trait::async_trait;
use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentRunAcceptedRefs, AgentRunCommandKind,
    AgentRunCommandReceiptRepository, LifecycleAgent, LifecycleAgentRepository, LifecycleRun,
    LifecycleRunRepository, RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::AgentConfig;
use agentdash_spi::platform::auth::AuthIdentity;

use crate::session::{LaunchCommand, SessionLaunchService, UserPromptInput};
use crate::workflow::{
    AgentRunCommandReceiptView, WorkflowApplicationError,
    command_receipt::{
        accepted_refs_from_record, claim_agent_run_command_receipt, digest_command_request,
        mark_command_terminal_failed,
    },
};

#[derive(Debug, Clone)]
pub struct AgentRunMessageCommand {
    pub delivery_runtime_session_id: String,
    pub input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub client_command_id: String,
    pub executor_config: Option<AgentConfig>,
    pub identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunMessageDispatch {
    pub runtime_session_id: String,
    pub turn_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
    pub command_receipt: AgentRunCommandReceiptView,
}

#[derive(Debug, Clone)]
pub struct AgentRunMessageDelivery {
    pub delivery_runtime_session_id: String,
    pub input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub executor_config: Option<AgentConfig>,
    pub identity: Option<AuthIdentity>,
}

#[async_trait]
pub trait AgentRunMessageDeliveryPort: Send + Sync {
    async fn deliver_user_message(
        &self,
        delivery: AgentRunMessageDelivery,
    ) -> Result<String, WorkflowApplicationError>;
}

#[derive(Clone)]
pub struct AgentRunMessageLaunchDeliveryPort {
    session_launch: SessionLaunchService,
}

impl AgentRunMessageLaunchDeliveryPort {
    pub fn new(session_launch: SessionLaunchService) -> Self {
        Self { session_launch }
    }
}

#[async_trait]
impl AgentRunMessageDeliveryPort for AgentRunMessageLaunchDeliveryPort {
    async fn deliver_user_message(
        &self,
        delivery: AgentRunMessageDelivery,
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
            .map_err(WorkflowApplicationError::from)
    }
}

pub struct AgentRunMessageService<'a, D> {
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    agent_frame_repo: &'a dyn AgentFrameRepository,
    execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    command_receipt_repo: &'a dyn AgentRunCommandReceiptRepository,
    delivery: D,
}

impl<'a, D> AgentRunMessageService<'a, D>
where
    D: AgentRunMessageDeliveryPort,
{
    pub fn new(
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
        agent_frame_repo: &'a dyn AgentFrameRepository,
        execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
        command_receipt_repo: &'a dyn AgentRunCommandReceiptRepository,
        delivery: D,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_agent_repo,
            agent_frame_repo,
            execution_anchor_repo,
            command_receipt_repo,
            delivery,
        }
    }

    pub async fn dispatch_user_message(
        &self,
        command: AgentRunMessageCommand,
    ) -> Result<AgentRunMessageDispatch, WorkflowApplicationError> {
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
        if command.client_command_id.trim().is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "client_command_id 不能为空".to_string(),
            ));
        }

        let (run, agent, _frame) = self
            .resolve_control_plane(&command.delivery_runtime_session_id)
            .await?;
        let request_digest = digest_command_request(&serde_json::json!({
            "kind": "agent_run_message",
            "run_id": run.id,
            "agent_id": agent.id,
            "runtime_session_id": command.delivery_runtime_session_id,
            "input": command.input,
            "executor_config": command.executor_config,
        }))?;
        let claim = claim_agent_run_command_receipt(
            self.command_receipt_repo,
            "agent_run_message",
            format!("{}:{}", run.id, agent.id),
            AgentRunCommandKind::MessageSubmit,
            command.client_command_id,
            request_digest,
        )
        .await?;
        if claim.duplicate {
            let accepted_refs = accepted_refs_from_record(&claim.record)?;
            return Ok(dispatch_from_accepted_refs(
                accepted_refs,
                AgentRunCommandReceiptView::from_record(&claim.record, true),
            ));
        }

        let delivery_result = self
            .delivery
            .deliver_user_message(AgentRunMessageDelivery {
                delivery_runtime_session_id: command.delivery_runtime_session_id.clone(),
                input: command.input,
                executor_config: command.executor_config,
                identity: command.identity,
            })
            .await;
        let turn_id = match delivery_result {
            Ok(turn_id) => turn_id,
            Err(error) => {
                mark_command_terminal_failed(self.command_receipt_repo, claim.record.id, &error)
                    .await;
                return Err(error);
            }
        };
        let (_accepted_run, _accepted_agent, accepted_frame) = self
            .resolve_control_plane(&command.delivery_runtime_session_id)
            .await?;
        let accepted_refs = AgentRunAcceptedRefs {
            run_id: run.id,
            agent_id: agent.id,
            frame_id: Some(accepted_frame.id),
            frame_revision: Some(accepted_frame.revision),
            runtime_session_id: Some(command.delivery_runtime_session_id.clone()),
            agent_run_turn_id: Some(turn_id.clone()),
            protocol_turn_id: None,
        };
        let receipt = self
            .command_receipt_repo
            .mark_accepted(claim.record.id, accepted_refs)
            .await?;

        Ok(AgentRunMessageDispatch {
            runtime_session_id: command.delivery_runtime_session_id,
            turn_id,
            run_id: run.id,
            agent_id: agent.id,
            frame_id: accepted_frame.id,
            frame_revision: accepted_frame.revision,
            command_receipt: AgentRunCommandReceiptView::from_record(&receipt, false),
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
        if is_terminal_agent_status(&agent.status) {
            return Err(WorkflowApplicationError::Conflict(
                "当前 Agent 已结束，不能继续发送消息".to_string(),
            ));
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

fn is_terminal_agent_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

fn dispatch_from_accepted_refs(
    refs: AgentRunAcceptedRefs,
    command_receipt: AgentRunCommandReceiptView,
) -> AgentRunMessageDispatch {
    let frame_id = refs.frame_id.unwrap_or_else(Uuid::nil);
    AgentRunMessageDispatch {
        runtime_session_id: refs.runtime_session_id.unwrap_or_default(),
        turn_id: refs.agent_run_turn_id.unwrap_or_default(),
        run_id: refs.run_id,
        agent_id: refs.agent_id,
        frame_id,
        frame_revision: refs.frame_revision.unwrap_or_default(),
        command_receipt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        AgentFrameRepository, AgentRunCommandClaim, AgentRunCommandReceipt,
        AgentRunCommandReceiptRepository, AgentRunCommandStatus, LifecycleAgentRepository,
        LifecycleRunRepository, NewAgentRunCommandReceipt, RuntimeSessionExecutionAnchor,
        RuntimeSessionExecutionAnchorRepository,
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
    struct InMemoryCommandReceiptRepo {
        items: Mutex<Vec<AgentRunCommandReceipt>>,
    }

    #[async_trait]
    impl AgentRunCommandReceiptRepository for InMemoryCommandReceiptRepo {
        async fn claim(
            &self,
            receipt: NewAgentRunCommandReceipt,
        ) -> Result<AgentRunCommandClaim, DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter().find(|item| {
                item.scope_kind == receipt.scope_kind
                    && item.scope_key == receipt.scope_key
                    && item.client_command_id == receipt.client_command_id
            }) {
                if existing.request_digest != receipt.request_digest {
                    return Err(DomainError::Conflict {
                        entity: "agent_run_command_receipt",
                        constraint: "request_digest",
                        message: "digest mismatch".to_string(),
                    });
                }
                return Ok(AgentRunCommandClaim::Duplicate(existing.clone()));
            }
            let now = Utc::now();
            let record = AgentRunCommandReceipt {
                id: Uuid::new_v4(),
                scope_kind: receipt.scope_kind,
                scope_key: receipt.scope_key,
                command_kind: receipt.command_kind,
                client_command_id: receipt.client_command_id,
                request_digest: receipt.request_digest,
                status: AgentRunCommandStatus::Pending,
                mailbox_message_id: None,
                accepted_refs: None,
                result_json: None,
                error_message: None,
                created_at: now,
                updated_at: now,
                accepted_at: None,
                failed_at: None,
            };
            items.push(record.clone());
            Ok(AgentRunCommandClaim::Created(record))
        }

        async fn mark_accepted(
            &self,
            id: Uuid,
            accepted_refs: agentdash_domain::workflow::AgentRunAcceptedRefs,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            let mut items = self.items.lock().unwrap();
            let record = items.iter_mut().find(|item| item.id == id).ok_or_else(|| {
                DomainError::NotFound {
                    entity: "agent_run_command_receipt",
                    id: id.to_string(),
                }
            })?;
            record.status = AgentRunCommandStatus::Accepted;
            record.accepted_refs = Some(accepted_refs);
            record.updated_at = Utc::now();
            record.accepted_at = Some(record.updated_at);
            Ok(record.clone())
        }

        async fn attach_mailbox_message(
            &self,
            id: Uuid,
            mailbox_message_id: Uuid,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            let mut items = self.items.lock().unwrap();
            let record = items.iter_mut().find(|item| item.id == id).ok_or_else(|| {
                DomainError::NotFound {
                    entity: "agent_run_command_receipt",
                    id: id.to_string(),
                }
            })?;
            record.mailbox_message_id = Some(mailbox_message_id);
            record.updated_at = Utc::now();
            Ok(record.clone())
        }

        async fn store_result_json(
            &self,
            id: Uuid,
            result_json: serde_json::Value,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            let mut items = self.items.lock().unwrap();
            let record = items.iter_mut().find(|item| item.id == id).ok_or_else(|| {
                DomainError::NotFound {
                    entity: "agent_run_command_receipt",
                    id: id.to_string(),
                }
            })?;
            record.result_json = Some(result_json);
            record.updated_at = Utc::now();
            Ok(record.clone())
        }

        async fn mark_terminal_failed(
            &self,
            id: Uuid,
            error_message: String,
        ) -> Result<AgentRunCommandReceipt, DomainError> {
            let mut items = self.items.lock().unwrap();
            let record = items.iter_mut().find(|item| item.id == id).ok_or_else(|| {
                DomainError::NotFound {
                    entity: "agent_run_command_receipt",
                    id: id.to_string(),
                }
            })?;
            record.status = AgentRunCommandStatus::TerminalFailed;
            record.error_message = Some(error_message);
            record.updated_at = Utc::now();
            record.failed_at = Some(record.updated_at);
            Ok(record.clone())
        }

        async fn get(&self, id: Uuid) -> Result<Option<AgentRunCommandReceipt>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.id == id)
                .cloned())
        }
    }

    #[derive(Default)]
    struct FakeDelivery {
        calls: Mutex<Vec<AgentRunMessageDelivery>>,
    }

    #[async_trait]
    impl AgentRunMessageDeliveryPort for &FakeDelivery {
        async fn deliver_user_message(
            &self,
            delivery: AgentRunMessageDelivery,
        ) -> Result<String, WorkflowApplicationError> {
            self.calls.lock().unwrap().push(delivery);
            Ok("turn-1".to_string())
        }
    }

    #[derive(Default)]
    struct FailingDelivery {
        calls: Mutex<usize>,
    }

    #[async_trait]
    impl AgentRunMessageDeliveryPort for &FailingDelivery {
        async fn deliver_user_message(
            &self,
            _delivery: AgentRunMessageDelivery,
        ) -> Result<String, WorkflowApplicationError> {
            *self.calls.lock().unwrap() += 1;
            Err(WorkflowApplicationError::Internal(
                "connector setup failed".to_string(),
            ))
        }
    }

    struct AdvancingDelivery<'a> {
        agent_repo: &'a InMemoryAgentRepo,
        frame_repo: &'a InMemoryFrameRepo,
        agent_id: Uuid,
    }

    #[async_trait]
    impl AgentRunMessageDeliveryPort for &AdvancingDelivery<'_> {
        async fn deliver_user_message(
            &self,
            delivery: AgentRunMessageDelivery,
        ) -> Result<String, WorkflowApplicationError> {
            let mut agent = self
                .agent_repo
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|agent| agent.id == self.agent_id)
                .cloned()
                .expect("seeded agent");
            let accepted_frame = AgentFrame::new_revision(self.agent_id, 2, "accepted");
            agent.set_current_frame(accepted_frame.id);
            self.frame_repo.items.lock().unwrap().push(accepted_frame);
            let mut agents = self.agent_repo.items.lock().unwrap();
            if let Some(existing) = agents.iter_mut().find(|item| item.id == agent.id) {
                *existing = agent;
            }
            assert_eq!(delivery.delivery_runtime_session_id, "runtime-1");
            Ok("turn-accepted".to_string())
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
        let mut run = LifecycleRun::new_control(project_id);
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
            ));
        (run, agent, frame)
    }

    #[tokio::test]
    async fn dispatch_user_message_resolves_anchor_and_delegates_delivery() {
        let run_repo = InMemoryRunRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let anchor_repo = InMemoryAnchorRepo::default();
        let command_receipt_repo = InMemoryCommandReceiptRepo::default();
        let delivery = FakeDelivery::default();
        let (run, agent, frame) = seed_control_plane(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            "runtime-1",
        );
        let service = AgentRunMessageService::new(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            &command_receipt_repo,
            &delivery,
        );

        let result = service
            .dispatch_user_message(AgentRunMessageCommand {
                delivery_runtime_session_id: "runtime-1".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-1".to_string(),
                executor_config: None,
                identity: None,
            })
            .await
            .expect("dispatch");

        assert_eq!(result.run_id, run.id);
        assert_eq!(result.agent_id, agent.id);
        assert_eq!(result.frame_id, frame.id);
        assert_eq!(result.runtime_session_id, "runtime-1");
        assert!(!result.command_receipt.duplicate);
        let calls = delivery.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].delivery_runtime_session_id, "runtime-1");
    }

    #[tokio::test]
    async fn duplicate_dispatch_returns_existing_receipt_without_delivery() {
        let run_repo = InMemoryRunRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let anchor_repo = InMemoryAnchorRepo::default();
        let command_receipt_repo = InMemoryCommandReceiptRepo::default();
        let delivery = FakeDelivery::default();
        seed_control_plane(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            "runtime-1",
        );
        let service = AgentRunMessageService::new(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            &command_receipt_repo,
            &delivery,
        );

        let first = service
            .dispatch_user_message(AgentRunMessageCommand {
                delivery_runtime_session_id: "runtime-1".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-1".to_string(),
                executor_config: None,
                identity: None,
            })
            .await
            .expect("first dispatch");
        let second = service
            .dispatch_user_message(AgentRunMessageCommand {
                delivery_runtime_session_id: "runtime-1".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-1".to_string(),
                executor_config: None,
                identity: None,
            })
            .await
            .expect("duplicate dispatch");

        assert_eq!(first.turn_id, second.turn_id);
        assert!(!first.command_receipt.duplicate);
        assert!(second.command_receipt.duplicate);
        assert_eq!(delivery.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn dispatch_user_message_rejects_terminal_agent_before_delivery() {
        let run_repo = InMemoryRunRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let anchor_repo = InMemoryAnchorRepo::default();
        let command_receipt_repo = InMemoryCommandReceiptRepo::default();
        let delivery = FakeDelivery::default();
        let (_run, mut agent, _frame) = seed_control_plane(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            "runtime-1",
        );
        agent.status = "completed".to_string();
        agent_repo.update(&agent).await.expect("update agent");
        let service = AgentRunMessageService::new(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            &command_receipt_repo,
            &delivery,
        );

        let error = service
            .dispatch_user_message(AgentRunMessageCommand {
                delivery_runtime_session_id: "runtime-1".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-terminal".to_string(),
                executor_config: None,
                identity: None,
            })
            .await
            .expect_err("terminal agent should reject message dispatch");

        assert!(matches!(error, WorkflowApplicationError::Conflict(_)));
        assert!(delivery.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dispatch_records_frame_after_delivery_acceptance() {
        let run_repo = InMemoryRunRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let anchor_repo = InMemoryAnchorRepo::default();
        let command_receipt_repo = InMemoryCommandReceiptRepo::default();
        let (_run, agent, launch_frame) = seed_control_plane(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            "runtime-1",
        );
        let delivery = AdvancingDelivery {
            agent_repo: &agent_repo,
            frame_repo: &frame_repo,
            agent_id: agent.id,
        };
        let service = AgentRunMessageService::new(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            &command_receipt_repo,
            &delivery,
        );

        let first = service
            .dispatch_user_message(AgentRunMessageCommand {
                delivery_runtime_session_id: "runtime-1".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-accepted".to_string(),
                executor_config: None,
                identity: None,
            })
            .await
            .expect("first dispatch");

        assert_ne!(first.frame_id, launch_frame.id);
        assert_eq!(first.frame_revision, 2);
        let duplicate = service
            .dispatch_user_message(AgentRunMessageCommand {
                delivery_runtime_session_id: "runtime-1".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-accepted".to_string(),
                executor_config: None,
                identity: None,
            })
            .await
            .expect("duplicate dispatch");
        assert!(duplicate.command_receipt.duplicate);
        assert_eq!(duplicate.frame_id, first.frame_id);
        assert_eq!(duplicate.frame_revision, 2);
    }

    #[tokio::test]
    async fn dispatch_user_message_uses_current_frame_when_anchor_launch_frame_is_stale() {
        let run_repo = InMemoryRunRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let anchor_repo = InMemoryAnchorRepo::default();
        let command_receipt_repo = InMemoryCommandReceiptRepo::default();
        let delivery = FakeDelivery::default();
        let (_run, mut agent, launch_frame) = seed_control_plane(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            "runtime-1",
        );
        let current_frame =
            AgentFrame::new_revision(agent.id, launch_frame.revision + 1, "current");
        agent.set_current_frame(current_frame.id);
        frame_repo.items.lock().unwrap().push(current_frame.clone());
        if let Some(stored_agent) = agent_repo
            .items
            .lock()
            .unwrap()
            .iter_mut()
            .find(|item| item.id == agent.id)
        {
            *stored_agent = agent.clone();
        }
        let service = AgentRunMessageService::new(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            &command_receipt_repo,
            &delivery,
        );

        let result = service
            .dispatch_user_message(AgentRunMessageCommand {
                delivery_runtime_session_id: "runtime-1".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-current-frame".to_string(),
                executor_config: None,
                identity: None,
            })
            .await
            .expect("dispatch");

        assert_eq!(result.frame_id, current_frame.id);
        assert_eq!(result.frame_revision, current_frame.revision);
        assert_ne!(
            result.frame_id, launch_frame.id,
            "message dispatch must record current AgentFrame, not stale launch-frame anchor"
        );
        assert_eq!(delivery.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn duplicate_dispatch_with_different_digest_conflicts() {
        let run_repo = InMemoryRunRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let anchor_repo = InMemoryAnchorRepo::default();
        let command_receipt_repo = InMemoryCommandReceiptRepo::default();
        let delivery = FakeDelivery::default();
        seed_control_plane(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            "runtime-1",
        );
        let service = AgentRunMessageService::new(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            &command_receipt_repo,
            &delivery,
        );

        service
            .dispatch_user_message(AgentRunMessageCommand {
                delivery_runtime_session_id: "runtime-1".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-1".to_string(),
                executor_config: None,
                identity: None,
            })
            .await
            .expect("first dispatch");
        let error = service
            .dispatch_user_message(AgentRunMessageCommand {
                delivery_runtime_session_id: "runtime-1".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("changed"),
                client_command_id: "cmd-1".to_string(),
                executor_config: None,
                identity: None,
            })
            .await
            .expect_err("digest mismatch");

        assert!(matches!(error, WorkflowApplicationError::Conflict(_)));
        assert_eq!(delivery.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn failed_dispatch_retry_returns_same_terminal_failure_without_delivery() {
        let run_repo = InMemoryRunRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let anchor_repo = InMemoryAnchorRepo::default();
        let command_receipt_repo = InMemoryCommandReceiptRepo::default();
        let delivery = FailingDelivery::default();
        seed_control_plane(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            "runtime-1",
        );
        let service = AgentRunMessageService::new(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            &command_receipt_repo,
            &delivery,
        );

        let command = || AgentRunMessageCommand {
            delivery_runtime_session_id: "runtime-1".to_string(),
            input: agentdash_agent_protocol::text_user_input_blocks("hello"),
            client_command_id: "cmd-1".to_string(),
            executor_config: None,
            identity: None,
        };
        let first = service
            .dispatch_user_message(command())
            .await
            .expect_err("first delivery fails");
        let second = service
            .dispatch_user_message(command())
            .await
            .expect_err("retry replays failure");

        assert!(matches!(first, WorkflowApplicationError::Internal(_)));
        assert!(matches!(second, WorkflowApplicationError::Conflict(_)));
        assert_eq!(*delivery.calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn dispatch_user_message_rejects_unresolved_runtime_session() {
        let run_repo = InMemoryRunRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let anchor_repo = InMemoryAnchorRepo::default();
        let command_receipt_repo = InMemoryCommandReceiptRepo::default();
        let delivery = FakeDelivery::default();
        let service = AgentRunMessageService::new(
            &run_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
            &command_receipt_repo,
            &delivery,
        );

        let error = service
            .dispatch_user_message(AgentRunMessageCommand {
                delivery_runtime_session_id: "missing".to_string(),
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-1".to_string(),
                executor_config: None,
                identity: None,
            })
            .await
            .expect_err("missing runtime session should fail");

        assert!(matches!(error, WorkflowApplicationError::NotFound(_)));
        assert!(delivery.calls.lock().unwrap().is_empty());
    }
}
