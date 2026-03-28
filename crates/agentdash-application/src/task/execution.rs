use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_domain::{
    common::ExecutorConfig,
    story::ChangeKind,
    task::{Task, TaskStatus},
};

#[derive(Debug, thiserror::Error)]
pub enum TaskExecutionError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    UnprocessableEntity(String),
    #[error("{0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPhase {
    Start,
    Continue,
}

#[derive(Debug, Clone)]
pub struct StartTaskCommand {
    pub task_id: Uuid,
    pub override_prompt: Option<String>,
    pub executor_config: Option<ExecutorConfig>,
}

#[derive(Debug, Clone)]
pub struct ContinueTaskCommand {
    pub task_id: Uuid,
    pub additional_prompt: Option<String>,
    pub executor_config: Option<ExecutorConfig>,
}

#[derive(Debug, Clone)]
pub struct StartTaskResult {
    pub task_id: Uuid,
    pub session_id: String,
    pub executor_session_id: Option<String>,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ContinueTaskResult {
    pub task_id: Uuid,
    pub session_id: String,
    pub executor_session_id: Option<String>,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SessionOverview {
    pub title: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct TaskSessionResult {
    pub task_id: Uuid,
    pub session_id: Option<String>,
    pub executor_session_id: Option<String>,
    pub task_status: TaskStatus,
    pub agent_binding: agentdash_domain::task::AgentBinding,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct StartedTurn {
    pub turn_id: String,
    pub context_sources: Vec<String>,
}

/// Legacy gateway trait — 已被 `TaskExecutionService` 取代。
///
/// 保留此 trait 仅为支持渐进迁移；新代码请使用 `super::service::TaskExecutionService`。
#[deprecated(note = "use TaskExecutionService instead")]
#[async_trait]
pub trait TaskExecutionGateway: Send + Sync {
    async fn get_task(&self, task_id: Uuid) -> Result<Task, TaskExecutionError>;
    async fn update_task(&self, task: &Task) -> Result<(), TaskExecutionError>;
    async fn get_backend_id_for_task(&self, task: &Task) -> Result<String, TaskExecutionError>;
    async fn append_task_change(
        &self,
        task_id: Uuid,
        backend_id: &str,
        kind: ChangeKind,
        payload: Value,
    ) -> Result<(), TaskExecutionError>;
    async fn create_task_session(&self, task: &Task) -> Result<String, TaskExecutionError>;
    async fn start_task_turn(
        &self,
        task: &Task,
        phase: ExecutionPhase,
        override_prompt: Option<&str>,
        additional_prompt: Option<&str>,
        executor_config: Option<ExecutorConfig>,
    ) -> Result<StartedTurn, TaskExecutionError>;
    async fn bind_session_to_owner(
        &self,
        session_id: &str,
        owner_type: &str,
        owner_id: Uuid,
        label: &str,
    ) -> Result<(), TaskExecutionError>;
    async fn cancel_session(&self, session_id: &str) -> Result<(), TaskExecutionError>;
    async fn get_session_overview(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionOverview>, TaskExecutionError>;
    async fn bridge_task_status_event_to_session(
        &self,
        session_id: &str,
        turn_id: &str,
        event_type: &str,
        message: &str,
        data: Value,
    );
    fn spawn_task_turn_monitor(
        &self,
        task_id: Uuid,
        session_id: String,
        turn_id: String,
        backend_id: String,
    );
}

#[allow(deprecated)]
pub async fn start_task<G: TaskExecutionGateway>(
    gateway: &G,
    cmd: StartTaskCommand,
) -> Result<StartTaskResult, TaskExecutionError> {
    let mut task = gateway.get_task(cmd.task_id).await?;

    if task.session_id.is_some() {
        return Err(TaskExecutionError::Conflict(
            "Task 已绑定 Session，请使用 continue 接口继续执行".into(),
        ));
    }
    if task.status == TaskStatus::Running {
        return Err(TaskExecutionError::Conflict("该任务已有执行进行中".into()));
    }

    let backend_id = gateway.get_backend_id_for_task(&task).await?;
    let session_id = gateway.create_task_session(&task).await?;
    let previous_status = task.status.clone();

    task.session_id = Some(session_id.clone());
    task.executor_session_id = None;
    task.status = TaskStatus::Running;
    gateway.update_task(&task).await?;

    if let Err(err) = gateway
        .bind_session_to_owner(&session_id, "task", task.id, "execution")
        .await
    {
        tracing::warn!(
            task_id = %task.id,
            session_id = %session_id,
            "写入 session_binding 失败（不阻塞执行流）: {}",
            err
        );
    }

    gateway
        .append_task_change(
            task.id,
            &backend_id,
            ChangeKind::TaskUpdated,
            json!({
                "reason": "task_session_bound",
                "task_id": task.id,
                "story_id": task.story_id,
                "session_id": task.session_id,
                "executor_session_id": task.executor_session_id,
            }),
        )
        .await?;

    if previous_status != task.status {
        gateway
            .append_task_change(
                task.id,
                &backend_id,
                ChangeKind::TaskStatusChanged,
                json!({
                    "reason": "task_start_accepted",
                    "task_id": task.id,
                    "story_id": task.story_id,
                    "session_id": task.session_id,
                    "executor_session_id": task.executor_session_id,
                    "from": previous_status.clone(),
                    "to": task.status.clone(),
                }),
            )
            .await?;
    }

    let started_turn = match gateway
        .start_task_turn(
            &task,
            ExecutionPhase::Start,
            cmd.override_prompt.as_deref(),
            None,
            cmd.executor_config,
        )
        .await
    {
        Ok(value) => value,
        Err(err) => {
            let mut fail_task = task.clone();
            fail_task.status = TaskStatus::Failed;
            let _ = gateway.update_task(&fail_task).await;
            let _ = gateway
                .append_task_change(
                    fail_task.id,
                    &backend_id,
                    ChangeKind::TaskStatusChanged,
                    json!({
                        "reason": "task_start_failed",
                        "task_id": fail_task.id,
                        "story_id": fail_task.story_id,
                        "session_id": fail_task.session_id,
                        "executor_session_id": fail_task.executor_session_id,
                        "from": TaskStatus::Running,
                        "to": TaskStatus::Failed,
                        "error": err.to_string(),
                    }),
                )
                .await;
            return Err(err);
        }
    };

    gateway
        .bridge_task_status_event_to_session(
            &session_id,
            &started_turn.turn_id,
            "task_start_accepted",
            "Task 已开始执行",
            json!({
                "task_id": task.id,
                "story_id": task.story_id,
                "session_id": session_id,
                "executor_session_id": task.executor_session_id,
                "from": previous_status,
                "to": task.status,
            }),
        )
        .await;

    gateway.spawn_task_turn_monitor(
        task.id,
        session_id.clone(),
        started_turn.turn_id.clone(),
        backend_id,
    );

    Ok(StartTaskResult {
        task_id: task.id,
        session_id,
        executor_session_id: task.executor_session_id.clone(),
        turn_id: started_turn.turn_id,
        status: task.status,
        context_sources: started_turn.context_sources,
    })
}

#[allow(deprecated)]
pub async fn continue_task<G: TaskExecutionGateway>(
    gateway: &G,
    cmd: ContinueTaskCommand,
) -> Result<ContinueTaskResult, TaskExecutionError> {
    let mut task = gateway.get_task(cmd.task_id).await?;
    let session_id = task.session_id.clone().ok_or_else(|| {
        TaskExecutionError::UnprocessableEntity("Task 尚未启动，请先执行 start".into())
    })?;

    if task.status == TaskStatus::Running {
        return Err(TaskExecutionError::Conflict("该任务已有执行进行中".into()));
    }

    let backend_id = gateway.get_backend_id_for_task(&task).await?;
    let started_turn = gateway
        .start_task_turn(
            &task,
            ExecutionPhase::Continue,
            None,
            cmd.additional_prompt.as_deref(),
            cmd.executor_config,
        )
        .await?;

    let previous_status = task.status.clone();
    task.status = TaskStatus::Running;
    gateway.update_task(&task).await?;

    if previous_status != task.status {
        gateway
            .append_task_change(
                task.id,
                &backend_id,
                ChangeKind::TaskStatusChanged,
                json!({
                    "reason": "task_continue_accepted",
                    "task_id": task.id,
                    "story_id": task.story_id,
                    "session_id": task.session_id,
                    "executor_session_id": task.executor_session_id,
                    "from": previous_status.clone(),
                    "to": task.status.clone(),
                }),
            )
            .await?;
    }

    gateway
        .bridge_task_status_event_to_session(
            &session_id,
            &started_turn.turn_id,
            "task_continue_accepted",
            "Task 已继续执行",
            json!({
                "task_id": task.id,
                "story_id": task.story_id,
                "session_id": session_id,
                "executor_session_id": task.executor_session_id,
                "from": previous_status,
                "to": task.status,
            }),
        )
        .await;

    gateway.spawn_task_turn_monitor(
        task.id,
        session_id.clone(),
        started_turn.turn_id.clone(),
        backend_id,
    );

    Ok(ContinueTaskResult {
        task_id: task.id,
        session_id,
        executor_session_id: task.executor_session_id.clone(),
        turn_id: started_turn.turn_id,
        status: task.status,
        context_sources: started_turn.context_sources,
    })
}

#[allow(deprecated)]
pub async fn cancel_task<G: TaskExecutionGateway>(
    gateway: &G,
    task_id: Uuid,
) -> Result<Task, TaskExecutionError> {
    let mut task = gateway.get_task(task_id).await?;
    let session_id = task.session_id.clone().ok_or_else(|| {
        TaskExecutionError::UnprocessableEntity("Task 尚未启动，无法取消执行".into())
    })?;

    gateway.cancel_session(&session_id).await?;

    if task.status == TaskStatus::Running {
        let previous_status = task.status.clone();
        task.status = TaskStatus::Failed;
        gateway.update_task(&task).await?;

        let backend_id = gateway
            .get_backend_id_for_task(&task)
            .await
            .unwrap_or_else(|_| "unknown".to_string());

        gateway
            .append_task_change(
                task.id,
                &backend_id,
                ChangeKind::TaskStatusChanged,
                json!({
                    "reason": "task_cancel_requested",
                    "task_id": task.id,
                    "story_id": task.story_id,
                    "session_id": task.session_id,
                    "executor_session_id": task.executor_session_id,
                    "from": previous_status,
                    "to": task.status,
                }),
            )
            .await?;
    }

    Ok(task)
}

#[allow(deprecated)]
pub async fn get_task_session<G: TaskExecutionGateway>(
    gateway: &G,
    task_id: Uuid,
) -> Result<TaskSessionResult, TaskExecutionError> {
    let task = gateway.get_task(task_id).await?;
    let session_id = task.session_id.clone();

    let (session_title, last_activity) = if let Some(value) = session_id.as_deref() {
        match gateway.get_session_overview(value).await? {
            Some(meta) => (Some(meta.title), Some(meta.updated_at)),
            None => (None, None),
        }
    } else {
        (None, None)
    };

    Ok(TaskSessionResult {
        task_id: task.id,
        session_id,
        executor_session_id: task.executor_session_id,
        task_status: task.status,
        agent_binding: task.agent_binding,
        session_title,
        last_activity,
    })
}
