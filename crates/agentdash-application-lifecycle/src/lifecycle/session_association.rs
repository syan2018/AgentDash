use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBinding, AgentRunRuntimeBindingRepository,
};
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, ExecutorRunRef, LifecycleAgent, LifecycleAgentRepository,
    LifecycleRun, LifecycleRunRepository, RuntimeNodeState,
};
use uuid::Uuid;

/// Lifecycle node 子 session 的 binding label 前缀。
pub const LIFECYCLE_NODE_LABEL_PREFIX: &str = "lifecycle_node:";
pub const LIFECYCLE_ACTIVITY_LABEL_PREFIX: &str = "lifecycle_activity:";

/// 子 session 与 lifecycle runtime node 的关联解析结果。
#[derive(Debug, Clone)]
pub struct LifecycleActivitySessionAssociation {
    pub run: LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

/// RuntimeSession terminal / advance 事件解析出的稳定 lifecycle execution 证据。
///
/// anchor 表达 runtime session 启动时的 launch frame evidence。
/// resolver 必须允许 runtime session 的当前 frame revision 演进后仍能回到 runtime node。
#[derive(Debug, Clone)]
pub struct ActivityRuntimeAssociation {
    pub run: LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

pub type RuntimeSessionCurrentFrame = (AgentRunRuntimeBinding, LifecycleAgent, AgentFrame);

/// 从 delivery trace ref 回溯当前 AgentFrame surface。
///
/// RuntimeSession 只作为 trace/delivery evidence；业务 owner 来自 anchor 反查出的
/// LifecycleAgent 与当前 AgentFrame。
pub async fn resolve_current_frame_from_delivery_trace_ref(
    runtime_session_id: &str,
    binding_repo: &dyn AgentRunRuntimeBindingRepository,
    agent_repo: &dyn LifecycleAgentRepository,
    frame_repo: &dyn AgentFrameRepository,
) -> Result<Option<RuntimeSessionCurrentFrame>, agentdash_domain::DomainError> {
    let thread_id = RuntimeThreadId::new(runtime_session_id).map_err(|error| {
        agentdash_domain::DomainError::Database {
            operation: "resolve_agent_run_runtime_binding",
            message: error.to_string(),
        }
    })?;
    let Some(binding) = binding_repo
        .load_by_thread_id(&thread_id)
        .await
        .map_err(|error| agentdash_domain::DomainError::Database {
            operation: "resolve_agent_run_runtime_binding",
            message: error.to_string(),
        })?
    else {
        return Ok(None);
    };
    let Some(agent) = agent_repo.get(binding.target.agent_id).await? else {
        return Ok(None);
    };
    if agent.run_id != binding.target.run_id {
        return Ok(None);
    }
    let Some(frame) = frame_repo.get_current(agent.id).await? else {
        return Ok(None);
    };
    if frame.agent_id != agent.id {
        return Ok(None);
    }
    Ok(Some((binding, agent, frame)))
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActivityRuntimeAssociationError {
    #[error("查询 {operation} 失败: {message}")]
    Repository {
        operation: &'static str,
        message: String,
    },
    #[error(
        "runtime session {runtime_session_id} 对应 AgentFrame {frame_id} 缺少 LifecycleAgent: agent_id={agent_id}"
    )]
    MissingLifecycleAgent {
        runtime_session_id: String,
        frame_id: Uuid,
        agent_id: Uuid,
    },
    #[error(
        "runtime session {runtime_session_id} 对应 AgentFrame {frame_id} 与 anchor agent 不匹配: agent_id={agent_id}"
    )]
    AnchorFrameMismatch {
        runtime_session_id: String,
        frame_id: Uuid,
        agent_id: Uuid,
    },
    #[error("runtime session {runtime_session_id} 指向的 lifecycle run 不存在: {run_id}")]
    MissingLifecycleRunForAnchor {
        runtime_session_id: String,
        run_id: Uuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleActivityLabelParts {
    pub run_id: Uuid,
    pub activity_key: String,
    pub attempt: u32,
}

/// 构造 lifecycle node 子 session 的 binding label。
pub fn build_lifecycle_node_label(node_key: &str) -> String {
    format!("{LIFECYCLE_NODE_LABEL_PREFIX}{node_key}")
}

pub fn build_lifecycle_activity_label(run_id: Uuid, activity_key: &str, attempt: u32) -> String {
    format!("{LIFECYCLE_ACTIVITY_LABEL_PREFIX}{run_id}:{activity_key}#{attempt}")
}

pub fn lifecycle_activity_parts_from_label(label: &str) -> Option<LifecycleActivityLabelParts> {
    let payload = label.strip_prefix(LIFECYCLE_ACTIVITY_LABEL_PREFIX)?.trim();
    let (run_id, activity_attempt) = payload.split_once(':')?;
    let (activity_key, attempt) = activity_attempt.rsplit_once('#')?;
    if activity_key.is_empty() {
        return None;
    }
    Some(LifecycleActivityLabelParts {
        run_id: Uuid::parse_str(run_id).ok()?,
        activity_key: activity_key.to_string(),
        attempt: attempt.parse().ok()?,
    })
}

pub struct ActivityRuntimeAssociationResolver<'a> {
    frame_repo: &'a dyn AgentFrameRepository,
    run_repo: &'a dyn LifecycleRunRepository,
    binding_repo: Option<&'a dyn AgentRunRuntimeBindingRepository>,
}

impl<'a> ActivityRuntimeAssociationResolver<'a> {
    pub fn new(
        frame_repo: &'a dyn AgentFrameRepository,
        run_repo: &'a dyn LifecycleRunRepository,
    ) -> Self {
        Self {
            frame_repo,
            run_repo,
            binding_repo: None,
        }
    }

    pub fn with_binding_repo(
        mut self,
        binding_repo: &'a dyn AgentRunRuntimeBindingRepository,
    ) -> Self {
        self.binding_repo = Some(binding_repo);
        self
    }

    pub async fn resolve_by_message_stream_trace(
        &self,
        session_id: &str,
    ) -> Result<Option<ActivityRuntimeAssociation>, ActivityRuntimeAssociationError> {
        let Some(binding_repo) = self.binding_repo else {
            return Ok(None);
        };
        let thread_id = RuntimeThreadId::new(session_id).map_err(|error| {
            ActivityRuntimeAssociationError::Repository {
                operation: "runtime thread id",
                message: error.to_string(),
            }
        })?;
        let Some(binding) = binding_repo
            .load_by_thread_id(&thread_id)
            .await
            .map_err(|e| ActivityRuntimeAssociationError::Repository {
                operation: "agent run runtime binding",
                message: e.to_string(),
            })?
        else {
            return Ok(None);
        };
        self.resolve_by_binding(session_id, binding).await
    }

    async fn resolve_by_binding(
        &self,
        session_id: &str,
        binding: AgentRunRuntimeBinding,
    ) -> Result<Option<ActivityRuntimeAssociation>, ActivityRuntimeAssociationError> {
        let Some(frame) = self
            .frame_repo
            .get_current(binding.target.agent_id)
            .await
            .map_err(|e| ActivityRuntimeAssociationError::Repository {
                operation: "current AgentFrame",
                message: e.to_string(),
            })?
        else {
            return Ok(None);
        };
        if frame.agent_id != binding.target.agent_id {
            return Err(ActivityRuntimeAssociationError::AnchorFrameMismatch {
                runtime_session_id: session_id.to_string(),
                frame_id: frame.id,
                agent_id: binding.target.agent_id,
            });
        }
        let run = self
            .run_repo
            .get_by_id(binding.target.run_id)
            .await
            .map_err(|e| ActivityRuntimeAssociationError::Repository {
                operation: "lifecycle run",
                message: e.to_string(),
            })?
            .ok_or(
                ActivityRuntimeAssociationError::MissingLifecycleRunForAnchor {
                    runtime_session_id: session_id.to_string(),
                    run_id: binding.target.run_id,
                },
            )?;
        let Some((orchestration_id, node_path, attempt)) =
            find_runtime_node_binding(&run, session_id)
        else {
            return Ok(None);
        };
        Ok(Some(ActivityRuntimeAssociation {
            run,
            orchestration_id,
            node_path,
            attempt,
        }))
    }
}

/// 从 message stream trace 解析 lifecycle runtime node 执行证据。
pub async fn resolve_activity_runtime_association_from_message_stream_trace(
    session_id: &str,
    frame_repo: &dyn AgentFrameRepository,
    _agent_repo: &dyn LifecycleAgentRepository,
    run_repo: &dyn LifecycleRunRepository,
    binding_repo: Option<&dyn AgentRunRuntimeBindingRepository>,
) -> Result<Option<LifecycleActivitySessionAssociation>, ActivityRuntimeAssociationError> {
    let mut resolver = ActivityRuntimeAssociationResolver::new(frame_repo, run_repo);
    if let Some(binding_repo) = binding_repo {
        resolver = resolver.with_binding_repo(binding_repo);
    }
    Ok(resolver
        .resolve_by_message_stream_trace(session_id)
        .await?
        .map(|association| {
            let ActivityRuntimeAssociation {
                run,
                orchestration_id,
                node_path,
                attempt,
                ..
            } = association;
            LifecycleActivitySessionAssociation {
                run,
                orchestration_id,
                node_path,
                attempt,
            }
        }))
}

fn find_runtime_node_binding(run: &LifecycleRun, session_id: &str) -> Option<(Uuid, String, u32)> {
    run.orchestrations.iter().find_map(|orchestration| {
        find_node_by_runtime_thread(&orchestration.node_tree, session_id).map(|node| {
            (
                orchestration.orchestration_id,
                node.node_path.clone(),
                node.attempt.max(1),
            )
        })
    })
}

fn find_node_by_runtime_thread<'a>(
    nodes: &'a [RuntimeNodeState],
    session_id: &str,
) -> Option<&'a RuntimeNodeState> {
    nodes.iter().find_map(|node| {
        if matches!(
            node.executor_run_ref.as_ref(),
            Some(ExecutorRunRef::RuntimeSession { session_id: bound }) if bound == session_id
        ) {
            return Some(node);
        }
        find_node_by_runtime_thread(&node.children, session_id)
    })
}
