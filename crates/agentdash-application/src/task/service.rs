use std::sync::Arc;

use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentPolicy, CapabilityPolicy, ContextPolicy, ExecutionIntent, ExecutionSource,
    LifecycleSubjectAssociationRepository, RunPolicy, RuntimePolicy, SubjectRef,
};

use crate::repository_set::RepositorySet;
use crate::task::lock::TaskLockMap;
use crate::workflow::dispatch_service::LifecycleDispatchService;
use crate::workflow::WorkflowApplicationError;

use super::execution::*;
use super::gateway::{get_task as gw_get_task, map_domain_error};

/// 基础设施回调 — 仅封装 Application 层无法直接完成的操作
///
/// 主要涉及：取消路由（通过 lifecycle agent/frame 路径）。
/// 由 API/Host 层提供具体实现。
#[async_trait::async_trait]
pub trait TurnDispatcher: Send + Sync {
    /// 取消会话执行（自动路由到本地 Hub 或远程中继）
    async fn cancel_session(&self, session_id: &str) -> Result<(), TaskExecutionError>;
}

/// Story step activation service — 通过 ExecutionIntent dispatch 编排 Task execution。
///
/// start/continue 构造 `ExecutionIntent(subject_ref=Task)` 提交给
/// `LifecycleDispatchService`，不再自行创建 session 或 binding。
pub struct StoryStepActivationService {
    pub repos: RepositorySet,
    pub dispatcher: Arc<dyn TurnDispatcher>,
    pub lock_map: Arc<TaskLockMap>,
}

impl StoryStepActivationService {
    pub async fn start_task(
        &self,
        cmd: TaskExecutionCommand,
    ) -> Result<TaskExecutionResult, TaskExecutionError> {
        debug_assert_eq!(cmd.phase, ExecutionPhase::Start);
        let svc = &self;
        svc.lock_map
            .with_lock(cmd.task_id, || async { svc.start_task_inner(cmd).await })
            .await
    }

    pub async fn continue_task(
        &self,
        cmd: TaskExecutionCommand,
    ) -> Result<TaskExecutionResult, TaskExecutionError> {
        debug_assert_eq!(cmd.phase, ExecutionPhase::Continue);
        let svc = &self;
        svc.lock_map
            .with_lock(cmd.task_id, || async { svc.continue_task_inner(cmd).await })
            .await
    }

    pub async fn cancel_task(&self, task_id: Uuid) -> Result<agentdash_domain::task::Task, TaskExecutionError> {
        self.lock_map
            .with_lock(task_id, || async { self.cancel_task_inner(task_id).await })
            .await
    }

    /// 查询 task 当前执行视图（lifecycle 投影）。
    pub async fn get_task_execution_view(
        &self,
        task_id: Uuid,
    ) -> Result<TaskExecutionView, TaskExecutionError> {
        let task = gw_get_task(&self.repos, task_id).await?;
        let refs = self.resolve_task_execution_refs(task_id).await?;

        let (execution_status, agent_ref, run_ref, frame_ref, trace_ref) =
            if let Some(refs) = refs {
                (Some("active".to_string()), Some(refs.agent_id), Some(refs.run_id), refs.frame_id, None)
            } else {
                (None, None, None, None, None)
            };

        Ok(TaskExecutionView {
            task_id: task.id,
            execution_status,
            agent_ref,
            run_ref,
            frame_ref,
            trace_ref,
            task_status: task.status().clone(),
        })
    }

    // ─── inner implementations ────────────────────────────────

    async fn start_task_inner(
        &self,
        cmd: TaskExecutionCommand,
    ) -> Result<TaskExecutionResult, TaskExecutionError> {
        let task = gw_get_task(&self.repos, cmd.task_id).await?;

        // 检查是否已有活跃 execution
        if self.resolve_task_execution_refs(task.id).await?.is_some() {
            return Err(TaskExecutionError::Conflict(
                "Task 已有活跃 execution，请使用 continue 接口继续执行".into(),
            ));
        }

        let intent = ExecutionIntent {
            project_id: task.project_id,
            source: ExecutionSource::User,
            subject_ref: Some(SubjectRef::new("task", task.id)),
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: None,
            agent_procedure_ref: None,
            run_policy: RunPolicy::CreateLinkedRun,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Isolated,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
            gate_policy: None,
        };

        let result = self.dispatch_intent(&intent).await?;

        Ok(TaskExecutionResult {
            task_id: task.id,
            run_ref: result.run_ref,
            agent_ref: result.agent_ref,
            frame_ref: result.frame_ref,
            trace_ref: result.trace_ref,
            status: task.status().clone(),
        })
    }

    async fn continue_task_inner(
        &self,
        cmd: TaskExecutionCommand,
    ) -> Result<TaskExecutionResult, TaskExecutionError> {
        let task = gw_get_task(&self.repos, cmd.task_id).await?;

        let refs = self
            .resolve_task_execution_refs(task.id)
            .await?
            .ok_or_else(|| {
                TaskExecutionError::UnprocessableEntity(
                    "Task 尚未启动，请先执行 start".into(),
                )
            })?;

        let intent = ExecutionIntent {
            project_id: task.project_id,
            source: ExecutionSource::User,
            subject_ref: Some(SubjectRef::new("task", task.id)),
            parent_run_id: Some(refs.run_id),
            parent_agent_id: Some(refs.agent_id),
            workflow_graph_ref: None,
            agent_procedure_ref: None,
            run_policy: RunPolicy::ReuseExisting,
            agent_policy: AgentPolicy::Resume,
            context_policy: ContextPolicy::Inherit,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
            gate_policy: None,
        };

        let result = self.dispatch_intent(&intent).await?;

        Ok(TaskExecutionResult {
            task_id: task.id,
            run_ref: result.run_ref,
            agent_ref: result.agent_ref,
            frame_ref: result.frame_ref,
            trace_ref: result.trace_ref,
            status: task.status().clone(),
        })
    }

    async fn cancel_task_inner(&self, task_id: Uuid) -> Result<agentdash_domain::task::Task, TaskExecutionError> {
        let task = gw_get_task(&self.repos, task_id).await?;

        let refs = self
            .resolve_task_execution_refs(task.id)
            .await?
            .ok_or_else(|| {
                TaskExecutionError::UnprocessableEntity("Task 尚未启动，无法取消执行".into())
            })?;

        // 通过 agent 当前 frame 查找 runtime session refs 执行 cancel
        let frame = self
            .repos
            .agent_frame_repo
            .get_current(refs.agent_id)
            .await
            .map_err(|e| TaskExecutionError::Internal(e.to_string()))?;

        if let Some(frame) = frame {
            if let Some(refs_json) = &frame.runtime_session_refs_json {
                if let Some(arr) = refs_json.as_array() {
                    if let Some(first) = arr.first().and_then(|v| v.as_str()) {
                        self.dispatcher.cancel_session(first).await?;
                    }
                }
            }
        }

        Ok(task)
    }

    // ─── private helpers ──────────────────────────────────────

    /// 通过 LifecycleSubjectAssociation 查找 task 的活跃 execution refs。
    async fn resolve_task_execution_refs(
        &self,
        task_id: Uuid,
    ) -> Result<Option<TaskExecutionRefs>, TaskExecutionError> {
        let subject = SubjectRef::new("task", task_id);
        let associations = self
            .repos
            .lifecycle_subject_association_repo
            .list_by_subject(&subject)
            .await
            .map_err(|e| TaskExecutionError::Internal(e.to_string()))?;

        let Some(assoc) = associations.first() else {
            return Ok(None);
        };

        // 从 association 获取 run_id，再查找 active agent
        let run_id = assoc.anchor_run_id;
        let agents = self
            .repos
            .lifecycle_agent_repo
            .list_by_run(run_id)
            .await
            .map_err(|e| TaskExecutionError::Internal(e.to_string()))?;

        let active_agent = agents.into_iter().find(|a| a.status == "active");
        let Some(agent) = active_agent else {
            return Ok(None);
        };

        let frame_id = agent.current_frame_id;

        Ok(Some(TaskExecutionRefs {
            run_id,
            agent_id: agent.id,
            frame_id,
        }))
    }

    /// 构造 LifecycleDispatchService 并 dispatch intent。
    async fn dispatch_intent(
        &self,
        intent: &ExecutionIntent,
    ) -> Result<agentdash_domain::workflow::ExecutionDispatchResult, TaskExecutionError> {
        let dispatch_service = LifecycleDispatchService::new(
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.workflow_graph_instance_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.agent_frame_repo.as_ref(),
            self.repos.lifecycle_subject_association_repo.as_ref(),
            self.repos.lifecycle_gate_repo.as_ref(),
            self.repos.agent_lineage_repo.as_ref(),
        );

        dispatch_service.dispatch(intent).await.map_err(map_workflow_error)
    }
}

/// 从 LifecycleSubjectAssociation 解析到的 Task execution 锚点引用。
#[derive(Debug, Clone)]
struct TaskExecutionRefs {
    run_id: Uuid,
    agent_id: Uuid,
    frame_id: Option<Uuid>,
}

fn map_workflow_error(err: WorkflowApplicationError) -> TaskExecutionError {
    match err {
        WorkflowApplicationError::BadRequest(msg) => TaskExecutionError::BadRequest(msg),
        WorkflowApplicationError::NotFound(msg) => TaskExecutionError::NotFound(msg),
        WorkflowApplicationError::Conflict(msg) => TaskExecutionError::Conflict(msg),
        WorkflowApplicationError::Internal(msg) => TaskExecutionError::Internal(msg),
    }
}
