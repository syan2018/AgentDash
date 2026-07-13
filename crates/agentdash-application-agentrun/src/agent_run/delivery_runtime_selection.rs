use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBindingError, AgentRunRuntimeBindingRepository, AgentRunRuntimeTarget,
};
use agentdash_application_ports::agent_run_surface::AgentRunRuntimeAddress;
use agentdash_application_ports::lifecycle_surface_projection::{
    MessageStreamProjectionRef, MessageStreamTraceKind,
};
use agentdash_domain::DomainError;
use agentdash_domain::workflow::{
    AgentFrameRepository, LifecycleAgent, LifecycleAgentRepository, LifecycleRunRepository,
};
use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

use crate::agent_run::{AgentRunExecutionState, AgentRunRuntime, AgentRunRuntimeError};
use crate::error::ApplicationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryRuntimeSelectionPolicy {
    CurrentDelivery { run_id: Uuid, agent_id: Uuid },
}

#[derive(Debug, Clone)]
pub struct DeliveryRuntimeSelection {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub current_frame_id: Uuid,
    pub launch_frame_id: Uuid,
    pub runtime_session_id: String,
    pub orchestration_id: Option<Uuid>,
    pub node_path: Option<String>,
    pub node_attempt: Option<u32>,
    pub observed_at: DateTime<Utc>,
    pub address: AgentRunRuntimeAddress,
    pub message_stream: MessageStreamProjectionRef,
    execution_state: AgentRunExecutionState,
}

impl DeliveryRuntimeSelection {
    pub fn execution_state(&self) -> AgentRunExecutionState {
        self.execution_state.clone()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DeliveryRuntimeSelectionError {
    #[error("LifecycleRun {run_id} 不存在")]
    RunNotFound { run_id: Uuid },
    #[error("LifecycleAgent {agent_id} 不存在")]
    AgentNotFound { agent_id: Uuid },
    #[error("LifecycleAgent {agent_id} 属于 run {actual_run_id}，不匹配请求 run {run_id}")]
    AgentRunMismatch {
        run_id: Uuid,
        agent_id: Uuid,
        actual_run_id: Uuid,
    },
    #[error("AgentRun {run_id}/LifecycleAgent {agent_id} 缺少 current delivery binding")]
    CurrentDeliveryMissing { run_id: Uuid, agent_id: Uuid },
    #[error("LifecycleAgent {agent_id} 缺少当前 AgentFrame revision")]
    CurrentFrameMissing { agent_id: Uuid },
    #[error("AgentFrame {frame_id} 不存在")]
    CurrentFrameNotFound { frame_id: Uuid },
    #[error("Runtime surface source_frame_id `{source_frame_id}` 不是合法 UUID")]
    InvalidSourceFrameId { source_frame_id: String },
    #[error("Runtime surface source AgentFrame {frame_id} 不存在")]
    LaunchFrameNotFound { frame_id: Uuid },
    #[error("terminal AgentRun runtime 缺少可展示的 turn identity")]
    TerminalTurnMissing,
    #[error("AgentRun runtime binding repository failed: {0}")]
    RuntimeBinding(#[from] AgentRunRuntimeBindingError),
    #[error("AgentRun runtime inspection failed: {0}")]
    Runtime(#[from] AgentRunRuntimeError),
    #[error(transparent)]
    Repository(#[from] DomainError),
}

impl From<DeliveryRuntimeSelectionError> for ApplicationError {
    fn from(error: DeliveryRuntimeSelectionError) -> Self {
        match error {
            DeliveryRuntimeSelectionError::RunNotFound { .. }
            | DeliveryRuntimeSelectionError::AgentNotFound { .. }
            | DeliveryRuntimeSelectionError::CurrentFrameNotFound { .. }
            | DeliveryRuntimeSelectionError::LaunchFrameNotFound { .. } => {
                ApplicationError::NotFound(error.to_string())
            }
            DeliveryRuntimeSelectionError::Repository(source) => ApplicationError::from(source),
            other => ApplicationError::Conflict(other.to_string()),
        }
    }
}

#[derive(Clone, Copy)]
pub struct DeliveryRuntimeSelectionRepositories<'a> {
    pub lifecycle_runs: &'a dyn LifecycleRunRepository,
    pub lifecycle_agents: &'a dyn LifecycleAgentRepository,
    pub agent_frames: &'a dyn AgentFrameRepository,
    pub runtime_bindings: &'a dyn AgentRunRuntimeBindingRepository,
}

pub struct DeliveryRuntimeSelectionService<'a> {
    repos: DeliveryRuntimeSelectionRepositories<'a>,
    runtime: &'a dyn AgentRunRuntime,
}

impl<'a> DeliveryRuntimeSelectionService<'a> {
    pub fn new(
        repos: DeliveryRuntimeSelectionRepositories<'a>,
        runtime: &'a dyn AgentRunRuntime,
    ) -> Self {
        Self { repos, runtime }
    }

    pub async fn select(
        &self,
        policy: DeliveryRuntimeSelectionPolicy,
    ) -> Result<DeliveryRuntimeSelection, DeliveryRuntimeSelectionError> {
        match policy {
            DeliveryRuntimeSelectionPolicy::CurrentDelivery { run_id, agent_id } => {
                self.select_current_delivery(run_id, agent_id).await
            }
        }
    }

    pub async fn select_current_delivery(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<DeliveryRuntimeSelection, DeliveryRuntimeSelectionError> {
        let run = self
            .repos
            .lifecycle_runs
            .get_by_id(run_id)
            .await?
            .ok_or(DeliveryRuntimeSelectionError::RunNotFound { run_id })?;
        let agent = self
            .repos
            .lifecycle_agents
            .get(agent_id)
            .await?
            .ok_or(DeliveryRuntimeSelectionError::AgentNotFound { agent_id })?;
        if agent.run_id != run_id {
            return Err(DeliveryRuntimeSelectionError::AgentRunMismatch {
                run_id,
                agent_id,
                actual_run_id: agent.run_id,
            });
        }
        let target = AgentRunRuntimeTarget { run_id, agent_id };
        let binding =
            self.repos.runtime_bindings.load(&target).await?.ok_or(
                DeliveryRuntimeSelectionError::CurrentDeliveryMissing { run_id, agent_id },
            )?;
        let current_frame = self
            .repos
            .agent_frames
            .get_current(agent_id)
            .await?
            .ok_or(DeliveryRuntimeSelectionError::CurrentFrameMissing { agent_id })?;
        if current_frame.agent_id != agent_id {
            return Err(DeliveryRuntimeSelectionError::CurrentFrameNotFound {
                frame_id: current_frame.id,
            });
        }
        let launch_frame_id = parse_source_frame_id(&binding.surface.source_frame_id)?;
        let launch_frame = self.repos.agent_frames.get(launch_frame_id).await?.ok_or(
            DeliveryRuntimeSelectionError::LaunchFrameNotFound {
                frame_id: launch_frame_id,
            },
        )?;
        if launch_frame.agent_id != agent_id {
            return Err(DeliveryRuntimeSelectionError::LaunchFrameNotFound {
                frame_id: launch_frame_id,
            });
        }
        let runtime = self.runtime.inspect(target).await?;
        let (execution_state, observed_at) = runtime_state(&agent, runtime.snapshot.as_ref())?;
        let runtime_session_id = binding.presentation_thread_id.to_string();
        let orchestration_coordinate =
            find_orchestration_coordinate(&run.orchestrations, &runtime_session_id);
        Ok(DeliveryRuntimeSelection {
            run_id,
            agent_id,
            current_frame_id: current_frame.id,
            launch_frame_id,
            runtime_session_id: runtime_session_id.clone(),
            orchestration_id: orchestration_coordinate.as_ref().map(|value| value.0),
            node_path: orchestration_coordinate
                .as_ref()
                .map(|value| value.1.clone()),
            node_attempt: orchestration_coordinate.map(|value| value.2),
            observed_at,
            address: AgentRunRuntimeAddress {
                run_id,
                agent_id,
                frame_id: current_frame.id,
            },
            message_stream: MessageStreamProjectionRef {
                runtime_session_id,
                trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
            },
            execution_state,
        })
    }
}

fn runtime_state(
    agent: &LifecycleAgent,
    snapshot: Option<&agentdash_agent_runtime_contract::RuntimeSnapshot>,
) -> Result<(AgentRunExecutionState, DateTime<Utc>), DeliveryRuntimeSelectionError> {
    let observed_at = snapshot
        .and_then(|snapshot| {
            Utc.timestamp_millis_opt(snapshot.captured_at_ms as i64)
                .single()
        })
        .unwrap_or(agent.updated_at);
    let active_turn_id = snapshot
        .and_then(|snapshot| snapshot.active_presentation_turn_id.as_ref())
        .map(ToString::to_string);
    let last_turn_id = active_turn_id.clone().or_else(|| {
        snapshot.and_then(|snapshot| {
            snapshot
                .transcript
                .iter()
                .rev()
                .map(|item| item.source_turn_id.trim())
                .find(|turn_id| !turn_id.is_empty())
                .map(ToOwned::to_owned)
        })
    });
    let state = if snapshot.is_some_and(|snapshot| {
        snapshot.status == agentdash_agent_runtime_contract::RuntimeThreadStatus::Lost
    }) || agent.status == "lost"
    {
        AgentRunExecutionState::Lost {
            turn_id: last_turn_id,
            message: None,
        }
    } else if active_turn_id.is_some() {
        AgentRunExecutionState::Running {
            turn_id: active_turn_id,
        }
    } else {
        match agent.status.as_str() {
            "running" => AgentRunExecutionState::Running { turn_id: None },
            "cancelling" => AgentRunExecutionState::Cancelling {
                turn_id: last_turn_id,
            },
            "completed" => AgentRunExecutionState::Completed {
                turn_id: required_terminal_turn(last_turn_id.clone())?,
            },
            "failed" => AgentRunExecutionState::Failed {
                turn_id: required_terminal_turn(last_turn_id.clone())?,
                message: None,
            },
            "interrupted" | "cancelled" | "canceled" => AgentRunExecutionState::Interrupted {
                turn_id: last_turn_id,
                message: None,
            },
            _ => AgentRunExecutionState::Idle,
        }
    };
    Ok((state, observed_at))
}

fn parse_source_frame_id(source_frame_id: &str) -> Result<Uuid, DeliveryRuntimeSelectionError> {
    Uuid::parse_str(source_frame_id).map_err(|_| {
        DeliveryRuntimeSelectionError::InvalidSourceFrameId {
            source_frame_id: source_frame_id.to_string(),
        }
    })
}

fn required_terminal_turn(
    turn_id: Option<String>,
) -> Result<String, DeliveryRuntimeSelectionError> {
    turn_id.ok_or(DeliveryRuntimeSelectionError::TerminalTurnMissing)
}

fn find_runtime_node_coordinate(
    nodes: &[agentdash_domain::workflow::RuntimeNodeState],
    runtime_session_id: &str,
) -> Option<(String, u32)> {
    for node in nodes {
        if matches!(
            node.executor_run_ref.as_ref(),
            Some(agentdash_domain::workflow::ExecutorRunRef::RuntimeSession { session_id })
                if session_id == runtime_session_id
        ) {
            return Some((node.node_path.clone(), node.attempt));
        }
        if let Some(found) = find_runtime_node_coordinate(&node.children, runtime_session_id) {
            return Some(found);
        }
    }
    None
}

fn find_orchestration_coordinate(
    orchestrations: &[agentdash_domain::workflow::OrchestrationInstance],
    runtime_session_id: &str,
) -> Option<(Uuid, String, u32)> {
    orchestrations.iter().find_map(|orchestration| {
        find_runtime_node_coordinate(&orchestration.node_tree, runtime_session_id)
            .map(|(path, attempt)| (orchestration.orchestration_id, path, attempt))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../tests/fixtures/session-parity/workspace-selection.json"
        ))
        .expect("pinned Main workspace selection fixture")
    }

    #[test]
    fn adopted_surface_uses_typed_source_frame_identity() {
        let fixture = fixture();
        let source = Uuid::parse_str(
            fixture["adopted_surface"]["source_frame_id"]
                .as_str()
                .expect("source frame fixture"),
        )
        .unwrap();
        let current = Uuid::parse_str(
            fixture["adopted_surface"]["current_frame_id"]
                .as_str()
                .expect("current frame fixture"),
        )
        .unwrap();
        assert_eq!(parse_source_frame_id(&source.to_string()).unwrap(), source);
        assert_ne!(
            source, current,
            "surface adoption must preserve the source frame"
        );
        assert!(matches!(
            parse_source_frame_id("not-a-frame"),
            Err(DeliveryRuntimeSelectionError::InvalidSourceFrameId { .. })
        ));
    }

    #[test]
    fn workflow_node_coordinate_matches_presentation_runtime_session() {
        let fixture = fixture();
        let runtime_session_id = fixture["workflow_node"]["runtime_session_id"]
            .as_str()
            .unwrap();
        let driver_thread_id = fixture["workflow_node"]["driver_thread_id"]
            .as_str()
            .unwrap();
        let node_path = fixture["workflow_node"]["node_path"].as_str().unwrap();
        let attempt = fixture["workflow_node"]["attempt"].as_u64().unwrap() as u32;
        let nodes: Vec<agentdash_domain::workflow::RuntimeNodeState> =
            serde_json::from_value(serde_json::json!([{
                "node_id": "root",
                "node_path": "root",
                "kind": "phase",
                "status": "running",
                "attempt": 1,
                "children": [{
                    "node_id": "agent",
                    "node_path": node_path,
                    "kind": "agent_call",
                    "status": "running",
                    "attempt": attempt,
                    "executor_run_ref": {
                        "kind": "runtime_session",
                        "session_id": runtime_session_id
                    }
                }]
            }]))
            .expect("workflow node fixture");
        assert_eq!(
            find_runtime_node_coordinate(&nodes, runtime_session_id),
            Some((node_path.to_string(), attempt))
        );
        assert_eq!(find_runtime_node_coordinate(&nodes, driver_thread_id), None);
    }

    #[test]
    fn terminal_state_rejects_missing_presentation_turn_identity() {
        let fixture = fixture();
        let turn_id = fixture["terminal"]["turn_id"].as_str().unwrap();
        assert_eq!(
            required_terminal_turn(Some(turn_id.to_string())).unwrap(),
            turn_id
        );
        assert!(matches!(
            required_terminal_turn(None),
            Err(DeliveryRuntimeSelectionError::TerminalTurnMissing)
        ));
    }
}
