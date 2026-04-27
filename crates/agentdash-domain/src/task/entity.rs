use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{AgentBinding, Artifact, TaskExecutionMode, TaskStatus};

/// Task — 用户工作项与 Session 策略壳
///
/// 面向用户展示的工作项容器，承载归属关系、独立业务状态机、Session 默认执行策略和结果摘要。
/// 真实执行在 Session 中发生；Task 通过 workspace_id 外键关联逻辑工作空间。
///
/// Session 归属关系通过 `SessionBinding` 管理（owner_type=task, label="execution"），
/// Task entity 不再持有 session_id。
///
/// **M2：投影字段可见性规则**（见 `.trellis/spec/backend/story-task-runtime.md` §2.4）
///
/// - `status` / `artifacts` 是 LifecycleStepState 的**只读投影**；外部代码不可直写。
/// - 字段设置为 `pub(crate)`（限 domain crate 内部可见），仅通过 [`Task::apply_projection`]
///   与聚合层 [`Story::apply_task_projection`] 经由 projector 修改。
/// - spec 字段（title / description / agent_binding / workspace_id 等）保持公开可写；
///   但 `Story::update_task` 的 closure 签名通过 [`TaskSpecMut`] 收紧，仅允许改 spec。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub project_id: Uuid,
    pub story_id: Uuid,
    /// 关联的 Workspace（外键，替代原 workspace_path 字符串）
    pub workspace_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    /// 执行状态：只读投影字段，外部不可直写（M2）。
    pub(crate) status: TaskStatus,
    /// 执行器原生会话 ID（用于 follow-up / resume）。
    ///
    /// 注意：这不是 AgentDash 内部的 `session_id`。
    /// AgentDash 内部会话归属统一通过
    /// `SessionBinding(owner_type=Task, label="execution")` 解析。
    pub executor_session_id: Option<String>,
    /// 执行模式 — 控制失败后的自动处理策略
    pub execution_mode: TaskExecutionMode,
    /// 结构化 Agent 绑定信息
    pub agent_binding: AgentBinding,
    /// 结构化执行产物列表：只读投影字段，外部不可直写（M2）。
    pub(crate) artifacts: Vec<Artifact>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    pub fn new(project_id: Uuid, story_id: Uuid, title: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            story_id,
            workspace_id: None,
            title,
            description,
            status: TaskStatus::Pending,
            executor_session_id: None,
            execution_mode: TaskExecutionMode::default(),
            agent_binding: AgentBinding::default(),
            artifacts: vec![],
            created_at: now,
            updated_at: now,
        }
    }

    /// 只读访问投影状态。
    pub fn status(&self) -> &TaskStatus {
        &self.status
    }

    /// 只读访问投影产物。
    pub fn artifacts(&self) -> &[Artifact] {
        &self.artifacts
    }

    /// M2 projection：将 LifecycleStepState 的状态投射到 Task。
    ///
    /// 状态映射规则：
    /// - `Pending` → `TaskStatus::Pending`
    /// - `Ready`   → `TaskStatus::Assigned`
    /// - `Running` → `TaskStatus::Running`
    /// - `Completed` → `TaskStatus::AwaitingVerification`
    ///   （Task 完成态由业务（hook / verification）进一步推进为 Completed/Failed）
    /// - `Failed`  → `TaskStatus::Failed`
    /// - `Skipped` → `TaskStatus::Completed`（跳过视为终态完成）
    ///
    /// 返回 `true` 表示状态发生变化，`false` 表示投影后状态不变。
    ///
    /// 仅 domain crate 内部可调用；应用层通过 `Story::apply_task_projection` 间接触达。
    pub(crate) fn apply_projection(
        &mut self,
        step_status: crate::workflow::LifecycleStepExecutionStatus,
    ) -> bool {
        use crate::workflow::LifecycleStepExecutionStatus as S;

        let next = match step_status {
            S::Pending => TaskStatus::Pending,
            S::Ready => TaskStatus::Assigned,
            S::Running => TaskStatus::Running,
            S::Completed => TaskStatus::AwaitingVerification,
            S::Failed => TaskStatus::Failed,
            S::Skipped => TaskStatus::Completed,
        };

        if self.status == next {
            return false;
        }
        self.status = next;
        self.updated_at = Utc::now();
        true
    }

    /// 直接设置 status —— **仅命令型路径使用**（MCP/API 主动标记）。
    ///
    /// 运行时投影路径必须调用 [`Task::apply_projection`] / [`crate::story::Story::apply_task_projection`]。
    /// 本方法保留的原因是 Rust 可见性无法只向"项目内部 application/mcp/api"开放；
    /// 它与 [`crate::story::Story::force_set_task_status`] 一起构成受控命令入口。
    pub fn set_status(&mut self, status: TaskStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }

    /// 追加一条 artifact —— **仅命令型路径使用**（MCP/API 主动上报）。
    ///
    /// 运行时 artifact 累积也目前走此入口（projector 下沉待后续 cleanup）。
    pub fn push_artifact(&mut self, artifact: Artifact) {
        self.artifacts.push(artifact);
        self.updated_at = Utc::now();
    }

    /// 对 artifacts 的受控可变访问 —— 仅 upsert 场景使用。
    pub fn artifacts_mut(&mut self) -> &mut Vec<Artifact> {
        &mut self.artifacts
    }

    // 保留 pub(crate) setters 供 domain crate 内部 projector / Story 桥接使用。
    pub(crate) fn set_status_internal(&mut self, status: TaskStatus) {
        self.set_status(status);
    }

    pub(crate) fn push_artifact_internal(&mut self, artifact: Artifact) {
        self.push_artifact(artifact);
    }

    pub(crate) fn artifacts_internal_mut(&mut self) -> &mut Vec<Artifact> {
        self.artifacts_mut()
    }
}

/// Task spec（可编辑字段）的可变视图，交给 `Story::update_task` closure 使用。
///
/// 通过仅暴露 spec 字段的 `&mut` 引用，保证 closure 内部无法写到投影字段
/// （`status` / `artifacts`）。见 `.trellis/spec/backend/story-task-runtime.md` §2.4。
pub struct TaskSpecMut<'a> {
    pub id: Uuid,
    pub project_id: Uuid,
    pub story_id: Uuid,
    pub title: &'a mut String,
    pub description: &'a mut String,
    pub workspace_id: &'a mut Option<Uuid>,
    pub executor_session_id: &'a mut Option<String>,
    pub execution_mode: &'a mut TaskExecutionMode,
    pub agent_binding: &'a mut AgentBinding,
    pub updated_at: &'a mut DateTime<Utc>,
}

impl<'a> TaskSpecMut<'a> {
    /// 从 `&mut Task` 构造视图。仅 domain crate 内部使用。
    pub(crate) fn from_task(task: &'a mut Task) -> Self {
        Self {
            id: task.id,
            project_id: task.project_id,
            story_id: task.story_id,
            title: &mut task.title,
            description: &mut task.description,
            workspace_id: &mut task.workspace_id,
            executor_session_id: &mut task.executor_session_id,
            execution_mode: &mut task.execution_mode,
            agent_binding: &mut task.agent_binding,
            updated_at: &mut task.updated_at,
        }
    }
}
