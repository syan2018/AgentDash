use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use agentdash_application_agentrun::agent_run::AgentRunTerminalRegistry;
use agentdash_domain::DomainError;
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxMessage, AgentRunMailboxRepository, ConsumptionBarrier, MailboxDelivery,
    MailboxDrainMode, MailboxMessageStatus, NewAgentRunMailboxMessage,
};
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, LifecycleAgent, LifecycleAgentRepository, LifecycleGate,
    LifecycleGateRepository, RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::ExecutionContext;
use agentdash_spi::connector::RuntimeToolProvider;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::types::ResolvedWaitScope;
use super::*;

#[tokio::test]
async fn wait_timeout_keeps_running_exec_activity_alive() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    terminal_registry.register_terminal_with_metadata(
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
        "term-1",
        "backend-1",
        None,
        Some("main"),
        Some("D:/repo"),
        Some("interactive"),
    );
    terminal_registry.update_state("term-1", "running", None);

    let service = test_service(terminal_registry.clone());
    let result = service
        .wait(
            WaitToolContext {
                delivery_runtime_session_id: Some("runtime-1".to_string()),
                turn_id: "turn-1".to_string(),
            },
            WaitActivityRequest {
                activity_refs: vec!["term-1".to_string()],
                kinds: vec!["exec".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    assert!(result.timed_out);
    assert_eq!(result.items[0].status, "running");
    assert_eq!(
        terminal_registry
            .get_terminal("term-1")
            .expect("terminal")
            .state,
        "running"
    );
}

#[tokio::test]
async fn wait_returns_completed_exec_with_shell_exec_next_ref() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    terminal_registry.register_terminal_with_metadata(
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
        "term-1",
        "backend-1",
        None,
        Some("main"),
        Some("D:/repo"),
        Some("interactive"),
    );
    terminal_registry.update_state("term-1", "exited", Some(0));

    let service = test_service(terminal_registry);
    let result = service
        .wait(
            WaitToolContext {
                delivery_runtime_session_id: Some("runtime-1".to_string()),
                turn_id: "turn-1".to_string(),
            },
            WaitActivityRequest {
                activity_refs: vec!["term-1".to_string()],
                kinds: vec!["exec".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    assert!(!result.timed_out);
    assert_eq!(result.items[0].status, "completed");
    assert_eq!(
        result.items[0]
            .next
            .as_ref()
            .and_then(|next| next.get("tool")),
        Some(&json!("shell_exec"))
    );
}

#[tokio::test]
async fn wait_returns_resolved_lifecycle_gate_activity() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    let gate_repo = Arc::new(MemoryGateRepo::default());
    let mut gate = LifecycleGate::open(
        Uuid::new_v4(),
        Some(Uuid::new_v4()),
        Some(Uuid::new_v4()),
        "companion_wait",
        "dispatch-1",
        Some(json!({ "summary": "done" })),
    );
    let gate_id = gate.id;
    gate.resolve("test");
    gate_repo.create(&gate).await.expect("create gate");

    let service = test_service_with_gate_repo(terminal_registry, gate_repo);
    let result = service
        .wait(
            WaitToolContext {
                delivery_runtime_session_id: Some("runtime-1".to_string()),
                turn_id: "turn-1".to_string(),
            },
            WaitActivityRequest {
                activity_refs: vec![gate_id.to_string()],
                kinds: vec!["subagent".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    assert!(!result.timed_out);
    assert_eq!(result.items[0].kind, "subagent");
    assert_eq!(result.items[0].status, "completed");
    assert_eq!(result.items[0].preview.as_deref(), Some("done"));
}

#[tokio::test]
async fn wait_uses_resolved_lifecycle_gate_payload_status() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    let gate_repo = Arc::new(MemoryGateRepo::default());
    let mut gate = LifecycleGate::open(
        Uuid::new_v4(),
        Some(Uuid::new_v4()),
        Some(Uuid::new_v4()),
        "companion_wait_follow_up",
        "dispatch-failed",
        Some(json!({
            "status": "failed",
            "summary": "provider model unsupported"
        })),
    );
    let gate_id = gate.id;
    gate.resolve("runtime_terminal");
    gate_repo.create(&gate).await.expect("create gate");

    let service = test_service_with_gate_repo(terminal_registry, gate_repo);
    let result = service
        .wait(
            WaitToolContext {
                delivery_runtime_session_id: Some("runtime-1".to_string()),
                turn_id: "turn-1".to_string(),
            },
            WaitActivityRequest {
                activity_refs: vec![gate_id.to_string()],
                kinds: vec!["subagent".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    assert!(!result.timed_out);
    assert_eq!(result.items[0].kind, "subagent");
    assert_eq!(result.items[0].status, "failed");
    assert_eq!(
        result.items[0].preview.as_deref(),
        Some("provider model unsupported")
    );
}

#[tokio::test]
async fn scoped_gate_wait_keeps_observed_gate_ref_after_resolution() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    let gate_repo = Arc::new(MemoryGateRepo::default());
    let run_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let frame_id = Uuid::new_v4();
    let mut gate = LifecycleGate::open(
        run_id,
        Some(agent_id),
        Some(frame_id),
        "companion_wait",
        "dispatch-1",
        Some(json!({ "summary": "waiting" })),
    );
    let gate_id = gate.id;
    gate_repo.create(&gate).await.expect("create gate");

    let service = test_service_with_gate_repo(terminal_registry, gate_repo.clone());
    let scope = ResolvedWaitScope {
        delivery_runtime_session_id: None,
        run_id: Some(run_id),
        agent_id: Some(agent_id),
        frame_id: Some(frame_id),
    };
    let request = WaitActivityRequest {
        activity_refs: Vec::new(),
        kinds: vec!["subagent".to_string()],
        timeout_ms: Some(10),
        max_items: Some(10),
        after_cursor: None,
    };
    let mut observed_refs = BTreeSet::new();

    let pending_items = service
        .collect_items(&scope, &request, &observed_refs)
        .await
        .expect("pending items");
    assert_eq!(pending_items[0].activity_ref, gate_id.to_string());
    assert_eq!(pending_items[0].status, "pending");
    observed_refs.extend(pending_items.into_iter().map(|item| item.activity_ref));

    gate.resolve("test");
    gate_repo.update(&gate).await.expect("resolve gate");

    let completed_items = service
        .collect_items(&scope, &request, &observed_refs)
        .await
        .expect("completed items");
    assert_eq!(completed_items[0].activity_ref, gate_id.to_string());
    assert_eq!(completed_items[0].status, "completed");
}

#[tokio::test]
async fn explicit_gate_ref_is_filtered_by_run_scope_not_current_agent_only() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    let gate_repo = Arc::new(MemoryGateRepo::default());
    let run_id = Uuid::new_v4();
    let parent_agent_id = Uuid::new_v4();
    let child_agent_id = Uuid::new_v4();
    let child_frame_id = Uuid::new_v4();
    let mut child_gate = LifecycleGate::open(
        run_id,
        Some(child_agent_id),
        Some(child_frame_id),
        "companion_wait",
        "dispatch-1",
        Some(json!({ "summary": "done" })),
    );
    let child_gate_id = child_gate.id;
    child_gate.resolve("test");
    gate_repo
        .create(&child_gate)
        .await
        .expect("create child gate");

    let mut other_run_gate = child_gate.clone();
    other_run_gate.id = Uuid::new_v4();
    other_run_gate.run_id = Uuid::new_v4();
    let other_run_gate_id = other_run_gate.id;
    gate_repo
        .create(&other_run_gate)
        .await
        .expect("create other run gate");

    let service = test_service_with_gate_repo(terminal_registry, gate_repo);
    let parent_scope = ResolvedWaitScope {
        delivery_runtime_session_id: None,
        run_id: Some(run_id),
        agent_id: Some(parent_agent_id),
        frame_id: Some(Uuid::new_v4()),
    };
    let request = WaitActivityRequest {
        activity_refs: vec![child_gate_id.to_string(), other_run_gate_id.to_string()],
        kinds: vec!["subagent".to_string()],
        timeout_ms: Some(10),
        max_items: Some(10),
        after_cursor: None,
    };

    let items = service
        .collect_items(&parent_scope, &request, &BTreeSet::new())
        .await
        .expect("items");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].activity_ref, child_gate_id.to_string());
}

#[tokio::test]
async fn wait_after_cursor_filters_older_items() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    terminal_registry.register_terminal_with_metadata(
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
        "term-1",
        "backend-1",
        None,
        Some("main"),
        Some("D:/repo"),
        Some("interactive"),
    );
    let created_at = terminal_registry
        .get_terminal("term-1")
        .expect("terminal")
        .created_at;

    let service = test_service(terminal_registry);
    let result = service
        .wait(
            WaitToolContext {
                delivery_runtime_session_id: Some("runtime-1".to_string()),
                turn_id: "turn-1".to_string(),
            },
            WaitActivityRequest {
                activity_refs: Vec::new(),
                kinds: vec!["exec".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: Some(created_at.to_string()),
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    assert!(result.timed_out);
    assert!(result.items.is_empty());
}

#[tokio::test]
async fn runtime_tool_catalog_includes_wait() {
    let provider =
        WaitRuntimeToolProvider::from_service(test_service(AgentRunTerminalRegistry::new()));
    let composer =
        crate::runtime_tools::provider::SessionRuntimeToolComposer::new(vec![Arc::new(provider)]);
    let context = ExecutionContext {
        session: agentdash_spi::ExecutionSessionFrame {
            turn_id: "runtime-1".to_string(),
            working_directory: std::path::PathBuf::from("."),
            environment_variables: std::collections::HashMap::new(),
            executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
            mcp_servers: Vec::new(),
            vfs: None,
            vfs_access_policy: None,
            backend_execution: None,
            runtime_backend_anchor: None,
            identity: None,
        },
        turn: agentdash_spi::ExecutionTurnFrame::default(),
    };

    let tools = composer.build_tools(&context).await.expect("build tools");

    assert!(tools.iter().any(|tool| tool.name() == "wait"));
}

fn test_service(terminal_registry: Arc<AgentRunTerminalRegistry>) -> WaitActivityService {
    test_service_with_gate_repo(terminal_registry, Arc::new(MemoryGateRepo::default()))
}

fn test_service_with_gate_repo(
    terminal_registry: Arc<AgentRunTerminalRegistry>,
    gate_repo: Arc<dyn LifecycleGateRepository>,
) -> WaitActivityService {
    WaitActivityService::from_repositories(
        Arc::new(NoopLifecycleAgentRepo),
        Arc::new(NoopAgentFrameRepo),
        Arc::new(NoopExecutionAnchorRepo),
        gate_repo,
        Arc::new(NoopMailboxRepo),
        terminal_registry,
    )
}

#[derive(Default)]
struct MemoryGateRepo {
    gates: Mutex<HashMap<Uuid, LifecycleGate>>,
}

#[async_trait]
impl LifecycleGateRepository for MemoryGateRepo {
    async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        self.gates.lock().unwrap().insert(gate.id, gate.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
        Ok(self.gates.lock().unwrap().get(&id).cloned())
    }

    async fn list_open_for_agent(&self, agent_id: Uuid) -> Result<Vec<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .unwrap()
            .values()
            .filter(|gate| gate.agent_id == Some(agent_id) && gate.is_open())
            .cloned()
            .collect())
    }

    async fn find_by_agent_and_correlation(
        &self,
        agent_id: Uuid,
        correlation_id: &str,
    ) -> Result<Option<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .unwrap()
            .values()
            .find(|gate| gate.agent_id == Some(agent_id) && gate.correlation_id == correlation_id)
            .cloned())
    }

    async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        self.gates.lock().unwrap().insert(gate.id, gate.clone());
        Ok(())
    }
}

struct NoopLifecycleAgentRepo;

#[async_trait]
impl LifecycleAgentRepository for NoopLifecycleAgentRepo {
    async fn create(&self, _agent: &LifecycleAgent) -> Result<(), DomainError> {
        Ok(())
    }

    async fn get(&self, _id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
        Ok(None)
    }

    async fn list_by_run(&self, _run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
        Ok(Vec::new())
    }

    async fn update(&self, _agent: &LifecycleAgent) -> Result<(), DomainError> {
        Ok(())
    }
}

struct NoopAgentFrameRepo;

#[async_trait]
impl AgentFrameRepository for NoopAgentFrameRepo {
    async fn create(&self, _frame: &AgentFrame) -> Result<(), DomainError> {
        Ok(())
    }

    async fn get(&self, _frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        Ok(None)
    }

    async fn get_current(&self, _agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        Ok(None)
    }

    async fn list_by_agent(&self, _agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
        Ok(Vec::new())
    }
}

struct NoopExecutionAnchorRepo;

#[async_trait]
impl RuntimeSessionExecutionAnchorRepository for NoopExecutionAnchorRepo {
    async fn create_once(
        &self,
        _anchor: &agentdash_domain::workflow::RuntimeSessionExecutionAnchor,
    ) -> Result<(), DomainError> {
        Ok(())
    }

    async fn delete_by_session(&self, _runtime_session_id: &str) -> Result<(), DomainError> {
        Ok(())
    }

    async fn find_by_session(
        &self,
        _runtime_session_id: &str,
    ) -> Result<Option<agentdash_domain::workflow::RuntimeSessionExecutionAnchor>, DomainError>
    {
        Ok(None)
    }

    async fn list_by_run(
        &self,
        _run_id: Uuid,
    ) -> Result<Vec<agentdash_domain::workflow::RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(Vec::new())
    }

    async fn list_by_agent(
        &self,
        _agent_id: Uuid,
    ) -> Result<Vec<agentdash_domain::workflow::RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(Vec::new())
    }

    async fn list_by_project_session_ids(
        &self,
        _runtime_session_ids: &[String],
    ) -> Result<Vec<agentdash_domain::workflow::RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(Vec::new())
    }
}

struct NoopMailboxRepo;

#[async_trait]
impl AgentRunMailboxRepository for NoopMailboxRepo {
    async fn create_message(
        &self,
        _message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn create_message_idempotent(
        &self,
        _message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn get_message(&self, _id: Uuid) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        Ok(None)
    }

    async fn list_messages(
        &self,
        _run_id: Uuid,
        _agent_id: Uuid,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
        Ok(Vec::new())
    }

    async fn claim_next(
        &self,
        _request: agentdash_domain::agent_run_mailbox::AgentRunMailboxClaimRequest,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
        Ok(Vec::new())
    }

    async fn recover_expired_consuming(
        &self,
        _now: chrono::DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        Ok(0)
    }

    async fn mark_message_status(
        &self,
        _id: Uuid,
        _claim_token: Option<Uuid>,
        _status: MailboxMessageStatus,
        _accepted_agent_run_turn_id: Option<String>,
        _accepted_protocol_turn_id: Option<String>,
        _last_error: Option<String>,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn update_message_policy(
        &self,
        _id: Uuid,
        _delivery: MailboxDelivery,
        _barrier: ConsumptionBarrier,
        _drain_mode: MailboxDrainMode,
        _priority: i32,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn delete_message(
        &self,
        _id: Uuid,
    ) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        Ok(None)
    }

    async fn cleanup_user_payload(&self, _id: Uuid) -> Result<(), DomainError> {
        Ok(())
    }

    async fn pause_state(
        &self,
        _run_id: Uuid,
        _agent_id: Uuid,
        _runtime_session_id: Option<String>,
        _reason: String,
        _message: Option<String>,
    ) -> Result<agentdash_domain::agent_run_mailbox::AgentRunMailboxState, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn resume_state(
        &self,
        _run_id: Uuid,
        _agent_id: Uuid,
        _runtime_session_id: Option<String>,
    ) -> Result<agentdash_domain::agent_run_mailbox::AgentRunMailboxState, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn get_state(
        &self,
        _run_id: Uuid,
        _agent_id: Uuid,
    ) -> Result<Option<agentdash_domain::agent_run_mailbox::AgentRunMailboxState>, DomainError>
    {
        Ok(None)
    }

    async fn set_backend_selection_preference(
        &self,
        _run_id: Uuid,
        _agent_id: Uuid,
        _runtime_session_id: Option<String>,
        _preference: Value,
    ) -> Result<agentdash_domain::agent_run_mailbox::AgentRunMailboxState, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn move_message_after(
        &self,
        _id: Uuid,
        _after_id: Option<Uuid>,
        _run_id: Uuid,
        _agent_id: Uuid,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }
}
