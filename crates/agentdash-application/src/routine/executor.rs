use agentdash_diagnostics::{diag, Subsystem};
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use agentdash_domain::project::Project;
use agentdash_domain::routine::{Routine, RoutineDispatchRefs, RoutineExecution};
use agentdash_domain::workflow::SubjectExecutionDispatchResult;
use agentdash_domain::workspace::Workspace;

use crate::ApplicationError;
use crate::lifecycle::{LifecycleDispatchService, WorkflowApplicationError};
use crate::repository_set::RepositorySet;
use crate::workspace::BackendAvailability;

use super::dispatch::build_routine_execution_intent_with_reuse;
use super::reuse_resolver::LifecycleAgentReuseResolver;
use super::template::render_prompt_template;

#[derive(Debug, Clone, PartialEq, Eq)]
enum RoutineAdmissionError {
    Failed(String),
    Skipped(String),
}

impl RoutineAdmissionError {
    #[cfg(test)]
    fn reason(&self) -> &str {
        match self {
            RoutineAdmissionError::Failed(reason) | RoutineAdmissionError::Skipped(reason) => {
                reason
            }
        }
    }
}

/// Routine 执行器 — 统一处理三种触发源的 dispatch。
///
/// 执行流程：
/// 1. 从 Routine 表加载 Routine 定义
/// 2. 渲染 prompt 模板（Tera 插值）
/// 3. 解析绑定的 Project Agent 配置
/// 4. 构造 ExecutionIntent（DispatchStrategy → dispatch policy 映射）
/// 5. 调用 LifecycleDispatchService::dispatch() 创建 run/agent/frame
/// 6. 记录 RoutineExecution dispatch refs
pub struct RoutineExecutor {
    repos: RepositorySet,
    availability: Arc<dyn BackendAvailability>,
}

struct RoutineAgentContext {
    workspace: Option<Workspace>,
}

impl RoutineExecutor {
    pub fn new(repos: RepositorySet, availability: Arc<dyn BackendAvailability>) -> Self {
        Self {
            repos,
            availability,
        }
    }

    /// 定时触发入口 — 由 CronScheduler 调用
    pub async fn fire_scheduled(&self, routine_id: Uuid) -> Result<Uuid, ApplicationError> {
        self.fire(routine_id, "scheduled", None, None).await
    }

    /// Webhook 触发入口 — 由 API endpoint 调用
    pub async fn fire_webhook(
        &self,
        routine_id: Uuid,
        text: Option<&str>,
        payload: Option<serde_json::Value>,
    ) -> Result<Uuid, ApplicationError> {
        self.fire(routine_id, "webhook", text, payload.as_ref())
            .await
    }

    /// 插件触发入口 — 由 RoutineTriggerProvider 回调
    pub async fn fire_plugin(
        &self,
        routine_id: Uuid,
        trigger_source: &str,
        payload: serde_json::Value,
    ) -> Result<Uuid, ApplicationError> {
        self.fire(routine_id, trigger_source, None, Some(&payload))
            .await
    }

    /// 统一触发执行
    async fn fire(
        &self,
        routine_id: Uuid,
        trigger_source: &str,
        append_text: Option<&str>,
        payload: Option<&serde_json::Value>,
    ) -> Result<Uuid, ApplicationError> {
        let routine = self
            .repos
            .routine_repo
            .get_by_id(routine_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| ApplicationError::NotFound(format!("Routine {routine_id} 不存在")))?;

        if !routine.enabled {
            return Err(ApplicationError::BadRequest(format!(
                "Routine {} 已禁用",
                routine.name
            )));
        }

        let mut execution = RoutineExecution::new(routine_id, trigger_source);
        execution.trigger_payload = payload.cloned();

        self.repos
            .routine_execution_repo
            .create(&execution)
            .await
            .map_err(ApplicationError::from)?;

        let rendered = match render_prompt_template(
            &routine.prompt_template,
            trigger_source,
            &routine.name,
            &routine.project_id.to_string(),
            payload,
        ) {
            Ok(mut prompt) => {
                if let Some(text) = append_text {
                    prompt.push_str("\n\n");
                    prompt.push_str(text);
                }
                prompt
            }
            Err(err) => {
                execution.mark_failed(format!("模板渲染失败: {err}"));
                if let Err(update_err) = self.repos.routine_execution_repo.update(&execution).await
                {
                    diag!(Error, Subsystem::Cron,
        execution_id = %execution.id, error = %update_err, "更新 RoutineExecution（模板渲染失败）落库失败");
                }
                return Err(ApplicationError::InvalidConfig(err));
            }
        };

        let agent_context = match self.load_agent_context(&routine).await {
            Ok(agent_context) => agent_context,
            Err(err) => {
                let reason = format!("加载 Routine Agent 配置失败: {err}");
                execution.mark_failed(&reason);
                if let Err(update_err) = self.repos.routine_execution_repo.update(&execution).await
                {
                    diag!(Error, Subsystem::Cron,
        execution_id = %execution.id, error = %update_err, "更新 RoutineExecution（加载 Agent 配置失败）落库失败");
                }
                return Err(err);
            }
        };

        if let Err(admission) = check_workspace_dispatch_admission(
            self.availability.as_ref(),
            agent_context.workspace.as_ref(),
        )
        .await
        {
            match admission {
                RoutineAdmissionError::Failed(reason) => {
                    execution.mark_failed(&reason);
                    if let Err(update_err) =
                        self.repos.routine_execution_repo.update(&execution).await
                    {
                        diag!(Error, Subsystem::Cron,
        execution_id = %execution.id, error = %update_err, "更新 RoutineExecution（workspace 准入失败）落库失败");
                    }
                    return Err(ApplicationError::InvalidConfig(reason));
                }
                RoutineAdmissionError::Skipped(reason) => {
                    execution.mark_skipped(reason);
                    let exec_id = execution.id;
                    if let Err(update_err) =
                        self.repos.routine_execution_repo.update(&execution).await
                    {
                        diag!(Error, Subsystem::Cron,
        execution_id = %execution.id, error = %update_err, "更新 RoutineExecution（workspace 准入跳过）落库失败");
                    }
                    return Ok(exec_id);
                }
            }
        }

        match self
            .execute_with_dispatch(&routine, &rendered, &mut execution)
            .await
        {
            Ok(()) => {
                let mut updated_routine = routine;
                updated_routine.last_fired_at = Some(Utc::now());
                updated_routine.updated_at = Utc::now();
                if let Err(update_err) = self.repos.routine_repo.update(&updated_routine).await {
                    diag!(Error, Subsystem::Cron,
        routine_id = %updated_routine.id, error = %update_err, "更新 Routine（last_fired_at）落库失败");
                }

                let exec_id = execution.id;
                if let Err(update_err) = self.repos.routine_execution_repo.update(&execution).await
                {
                    diag!(Error, Subsystem::Cron,
        execution_id = %execution.id, error = %update_err, "更新 RoutineExecution（dispatch 完成）落库失败");
                }
                Ok(exec_id)
            }
            Err(err) => {
                execution.mark_failed(err.to_string());
                if let Err(update_err) = self.repos.routine_execution_repo.update(&execution).await
                {
                    diag!(Error, Subsystem::Cron,
        execution_id = %execution.id, error = %update_err, "更新 RoutineExecution（dispatch 失败）落库失败");
                }
                Err(err)
            }
        }
    }

    /// 通过 LifecycleDispatchService 执行 dispatch。
    async fn execute_with_dispatch(
        &self,
        routine: &Routine,
        prompt: &str,
        execution: &mut RoutineExecution,
    ) -> Result<(), ApplicationError> {
        let reuse_resolution = LifecycleAgentReuseResolver::from_repositories(&self.repos)
            .resolve(routine, execution)
            .await?;
        execution.entity_key = reuse_resolution.entity_key.clone();

        let intent = build_routine_execution_intent_with_reuse(
            routine,
            execution,
            reuse_resolution.target.as_ref(),
        );

        let dispatch_service = LifecycleDispatchService::new(
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.workflow_graph_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.agent_frame_repo.as_ref(),
            self.repos.lifecycle_subject_association_repo.as_ref(),
            self.repos.lifecycle_gate_repo.as_ref(),
            self.repos.agent_lineage_repo.as_ref(),
        )
        .with_anchor_repo(self.repos.execution_anchor_repo.as_ref())
        .with_runtime_session_creator(self.repos.runtime_session_creator.as_ref())
        .with_frame_construction_port(self.repos.agent_frame_construction.as_ref());

        let result: SubjectExecutionDispatchResult = dispatch_service
            .execute_subject(&intent)
            .await
            .map_err(map_routine_dispatch_error)?;

        let refs = RoutineDispatchRefs::new(result.runtime_refs.clone());

        execution.mark_dispatched(refs, prompt.to_string());

        diag!(Info, Subsystem::Cron,
            execution_id = %execution.id,
            run_id = %result.runtime_refs.run_ref,
            agent_id = %result.runtime_refs.agent_ref,
            frame_id = %result.runtime_refs.frame_ref,
            "Routine dispatch 成功"
        );

        Ok(())
    }

    async fn load_agent_context(
        &self,
        routine: &Routine,
    ) -> Result<RoutineAgentContext, ApplicationError> {
        let project = self
            .repos
            .project_repo
            .get_by_id(routine.project_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| {
                ApplicationError::NotFound(format!("Project {} 不存在", routine.project_id))
            })?;
        let workspace = resolve_project_workspace(&self.repos, &project).await?;
        let agent = self
            .repos
            .project_agent_repo
            .get_by_project_and_id(project.id, routine.project_agent_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| {
                ApplicationError::NotFound(format!(
                    "ProjectAgent {} 不存在",
                    routine.project_agent_id
                ))
            })?;

        agent
            .preset_config()
            .map_err(|error| ApplicationError::InvalidConfig(error.to_string()))?;

        Ok(RoutineAgentContext { workspace })
    }
}

async fn resolve_project_workspace(
    repos: &RepositorySet,
    project: &Project,
) -> Result<Option<Workspace>, ApplicationError> {
    match project.config.default_workspace_id {
        Some(workspace_id) => {
            let workspace = repos
                .workspace_repo
                .get_by_id(workspace_id)
                .await
                .map_err(ApplicationError::from)?
                .ok_or_else(|| {
                    ApplicationError::NotFound(format!("默认 Workspace {workspace_id} 不存在"))
                })?;
            Ok(Some(workspace))
        }
        None => Ok(None),
    }
}

async fn check_workspace_dispatch_admission(
    availability: &dyn BackendAvailability,
    workspace: Option<&Workspace>,
) -> Result<(), RoutineAdmissionError> {
    let Some(workspace) = workspace else {
        return Ok(());
    };
    if workspace.bindings.is_empty() {
        return Err(RoutineAdmissionError::Failed(format!(
            "Workspace `{}` 当前没有配置 backend binding，Routine 无法派发",
            workspace.name
        )));
    }

    let backend_ids = workspace
        .bindings
        .iter()
        .map(|binding| binding.backend_id.trim())
        .filter(|backend_id| !backend_id.is_empty())
        .collect::<std::collections::BTreeSet<_>>();
    if backend_ids.is_empty() {
        return Err(RoutineAdmissionError::Failed(format!(
            "Workspace `{}` 的 backend binding 缺少 backend_id，Routine 无法派发",
            workspace.name
        )));
    }

    for backend_id in &backend_ids {
        if availability.is_online(backend_id).await {
            return Ok(());
        }
    }

    Err(RoutineAdmissionError::Skipped(format!(
        "Workspace `{}` 当前没有在线 backend，Routine 本次触发已跳过：{}",
        workspace.name,
        backend_ids
            .into_iter()
            .map(|backend_id| format!("backend `{backend_id}` 离线"))
            .collect::<Vec<_>>()
            .join("；")
    )))
}

fn map_routine_dispatch_error(error: WorkflowApplicationError) -> ApplicationError {
    match error {
        WorkflowApplicationError::BadRequest(message) => {
            ApplicationError::BadRequest(format!("Routine dispatch 失败: {message}"))
        }
        WorkflowApplicationError::ModelRequired(message) => {
            ApplicationError::BadRequest(format!("Routine dispatch 失败: {message}"))
        }
        WorkflowApplicationError::NotFound(message) => {
            ApplicationError::NotFound(format!("Routine dispatch 失败: {message}"))
        }
        WorkflowApplicationError::Conflict(message) => {
            ApplicationError::Conflict(format!("Routine dispatch 失败: {message}"))
        }
        WorkflowApplicationError::Internal(message) => {
            ApplicationError::Internal(format!("Routine dispatch 失败: {message}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workspace::{
        WorkspaceBinding, WorkspaceIdentityKind, WorkspaceResolutionPolicy,
    };
    use async_trait::async_trait;

    struct MockAvailability {
        online_backend_ids: Vec<String>,
    }

    #[async_trait]
    impl BackendAvailability for MockAvailability {
        async fn is_online(&self, backend_id: &str) -> bool {
            self.online_backend_ids
                .iter()
                .any(|online| online == backend_id)
        }
    }

    fn routine_workspace(bindings: Vec<WorkspaceBinding>) -> Workspace {
        let mut workspace = Workspace::new(
            Uuid::new_v4(),
            "routine-ws".to_string(),
            WorkspaceIdentityKind::LocalDir,
            serde_json::json!({
                "match_mode": "path_key",
                "path_key": "routine-ws"
            }),
            WorkspaceResolutionPolicy::PreferOnline,
        );
        workspace.set_bindings(bindings);
        workspace
    }

    fn workspace_binding(backend_id: &str) -> WorkspaceBinding {
        WorkspaceBinding::new(
            Uuid::new_v4(),
            backend_id.to_string(),
            "/workspace".to_string(),
            serde_json::json!({ "binding_labels": {} }),
        )
    }

    #[tokio::test]
    async fn workspace_admission_allows_online_backend() {
        let workspace = routine_workspace(vec![workspace_binding("backend-a")]);
        let availability = MockAvailability {
            online_backend_ids: vec!["backend-a".to_string()],
        };

        check_workspace_dispatch_admission(&availability, Some(&workspace))
            .await
            .expect("online workspace should be dispatchable");
    }

    #[tokio::test]
    async fn workspace_admission_skips_when_all_backends_offline() {
        let workspace = routine_workspace(vec![workspace_binding("backend-a")]);
        let availability = MockAvailability {
            online_backend_ids: Vec::new(),
        };

        let err = check_workspace_dispatch_admission(&availability, Some(&workspace))
            .await
            .expect_err("offline workspace should be skipped");

        assert!(matches!(err, RoutineAdmissionError::Skipped(_)));
        assert!(err.reason().contains("已跳过"));
    }

    #[tokio::test]
    async fn workspace_admission_fails_when_binding_config_is_missing() {
        let workspace = routine_workspace(Vec::new());
        let availability = MockAvailability {
            online_backend_ids: Vec::new(),
        };

        let err = check_workspace_dispatch_admission(&availability, Some(&workspace))
            .await
            .expect_err("missing binding is a configuration failure");

        assert!(matches!(err, RoutineAdmissionError::Failed(_)));
        assert!(err.reason().contains("没有配置 backend binding"));
    }
}
