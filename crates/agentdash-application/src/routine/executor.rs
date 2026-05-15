use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use agentdash_domain::project::Project;
use agentdash_domain::routine::{Routine, RoutineExecution, SessionStrategy};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_domain::workspace::Workspace;
use agentdash_spi::AgentConnector;

use crate::context::SharedContextAuditBus;
use crate::repository_set::RepositorySet;
use crate::session::LaunchCommand;
use crate::session::SessionHub;
use crate::session::types::UserPromptInput;
use crate::vfs::RelayVfsService;
use crate::workspace::BackendAvailability;

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

/// Routine 执行器 — 统一处理三种触发源的 session 创建 / prompt 发送。
///
/// 执行流程：
/// 1. 从 Routine 表加载 Routine 定义
/// 2. 渲染 prompt 模板（Tera 插值）
/// 3. 解析绑定的 Project Agent 配置
/// 4. 根据 SessionStrategy 创建/复用 session
/// 5. 构造 owner-aware prompt request 并发送
/// 6. 记录 RoutineExecution
pub struct RoutineExecutor {
    repos: RepositorySet,
    session_hub: SessionHub,
    availability: Arc<dyn BackendAvailability>,
    audit_bus: Option<SharedContextAuditBus>,
}

struct RoutineAgentContext {
    workspace: Option<Workspace>,
}

impl RoutineExecutor {
    pub fn new(
        repos: RepositorySet,
        session_hub: SessionHub,
        _vfs_service: Arc<RelayVfsService>,
        _connector: Arc<dyn AgentConnector>,
        _platform_config: crate::platform_config::SharedPlatformConfig,
        availability: Arc<dyn BackendAvailability>,
    ) -> Self {
        Self {
            repos,
            session_hub,
            availability,
            audit_bus: None,
        }
    }

    /// 配置上下文审计总线（可选）。
    pub fn with_audit_bus(mut self, bus: SharedContextAuditBus) -> Self {
        self.audit_bus = Some(bus);
        self
    }

    /// 定时触发入口 — 由 CronScheduler 调用
    pub async fn fire_scheduled(&self, routine_id: Uuid) -> Result<Uuid, String> {
        self.fire(routine_id, "scheduled", None, None).await
    }

    /// Webhook 触发入口 — 由 API endpoint 调用
    pub async fn fire_webhook(
        &self,
        routine_id: Uuid,
        text: Option<&str>,
        payload: Option<serde_json::Value>,
    ) -> Result<Uuid, String> {
        self.fire(routine_id, "webhook", text, payload.as_ref())
            .await
    }

    /// 插件触发入口 — 由 RoutineTriggerProvider 回调
    pub async fn fire_plugin(
        &self,
        routine_id: Uuid,
        trigger_source: &str,
        payload: serde_json::Value,
    ) -> Result<Uuid, String> {
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
    ) -> Result<Uuid, String> {
        let routine = self
            .repos
            .routine_repo
            .get_by_id(routine_id)
            .await
            .map_err(|e| format!("查询 Routine 失败: {e}"))?
            .ok_or_else(|| format!("Routine {routine_id} 不存在"))?;

        if !routine.enabled {
            return Err(format!("Routine {} 已禁用", routine.name));
        }

        let mut execution = RoutineExecution::new(routine_id, trigger_source);
        execution.trigger_payload = payload.cloned();

        self.repos
            .routine_execution_repo
            .create(&execution)
            .await
            .map_err(|e| format!("创建执行记录失败: {e}"))?;

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
                let _ = self.repos.routine_execution_repo.update(&execution).await;
                return Err(err);
            }
        };

        let agent_context = match self.load_agent_context(&routine).await {
            Ok(agent_context) => agent_context,
            Err(err) => {
                let reason = format!("加载 Routine Agent 配置失败: {err}");
                execution.mark_failed(&reason);
                let _ = self.repos.routine_execution_repo.update(&execution).await;
                return Err(reason);
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
                    let _ = self.repos.routine_execution_repo.update(&execution).await;
                    return Err(reason);
                }
                RoutineAdmissionError::Skipped(reason) => {
                    execution.mark_skipped(reason);
                    let exec_id = execution.id;
                    let _ = self.repos.routine_execution_repo.update(&execution).await;
                    return Ok(exec_id);
                }
            }
        }

        match self
            .execute_with_session(&routine, &rendered, &mut execution)
            .await
        {
            Ok(()) => {
                let mut updated_routine = routine;
                updated_routine.last_fired_at = Some(Utc::now());
                updated_routine.updated_at = Utc::now();
                let _ = self.repos.routine_repo.update(&updated_routine).await;

                let exec_id = execution.id;
                let _ = self.repos.routine_execution_repo.update(&execution).await;
                Ok(exec_id)
            }
            Err(err) => {
                execution.mark_failed(&err);
                let _ = self.repos.routine_execution_repo.update(&execution).await;
                Err(err)
            }
        }
    }

    async fn execute_with_session(
        &self,
        routine: &Routine,
        prompt: &str,
        execution: &mut RoutineExecution,
    ) -> Result<(), String> {
        let session_id = self
            .resolve_session_id(routine, execution)
            .await
            .map_err(|err| format!("解析 Routine session 失败: {err}"))?;
        let command = LaunchCommand::routine_executor_input(
            UserPromptInput::from_text(prompt),
            Some(agentdash_spi::platform::auth::AuthIdentity::system_routine(
                routine.id,
            )),
        );

        execution.mark_running(&session_id, prompt.to_string());
        let _ = self.repos.routine_execution_repo.update(execution).await;

        let _turn_id = self
            .session_hub
            .launch_command(&session_id, command)
            .await
            .map_err(|e| format!("发送 prompt 失败: {e}"))?;

        // NOTE: mark_completed 追踪的是「prompt 已成功派发到 session」，
        // 而非「Agent 已执行完毕」。完整的 Agent 完成追踪需要 session turn 完成回调，
        // 当前阶段以 dispatch 级别审计为主，后续按需扩展。
        execution.mark_completed();

        Ok(())
    }

    async fn load_agent_context(&self, routine: &Routine) -> Result<RoutineAgentContext, String> {
        let project = self
            .repos
            .project_repo
            .get_by_id(routine.project_id)
            .await
            .map_err(|e| format!("查询 Project 失败: {e}"))?
            .ok_or_else(|| format!("Project {} 不存在", routine.project_id))?;
        let workspace = resolve_project_workspace(&self.repos, &project).await?;
        let agent = self
            .repos
            .agent_repo
            .get_by_id(routine.agent_id)
            .await
            .map_err(|e| format!("查询 Agent 失败: {e}"))?
            .ok_or_else(|| format!("Agent {} 不存在", routine.agent_id))?;
        let link = self
            .repos
            .agent_link_repo
            .find_by_project_and_agent(project.id, routine.agent_id)
            .await
            .map_err(|e| format!("查询 ProjectAgentLink 失败: {e}"))?
            .ok_or_else(|| {
                format!(
                    "Project {} 未关联 Agent {}，Routine 无法解析执行配置",
                    project.id, routine.agent_id
                )
            })?;

        link.merged_preset_config(&agent)
            .map_err(|error| error.to_string())?;

        Ok(RoutineAgentContext { workspace })
    }

    async fn resolve_session_id(
        &self,
        routine: &Routine,
        execution: &mut RoutineExecution,
    ) -> Result<String, String> {
        match &routine.session_strategy {
            SessionStrategy::Fresh => {
                let title = format!("Routine: {}", routine.name);
                let label = format!("routine:{}:execution:{}", routine.id, execution.id);
                self.create_project_owned_session(routine.project_id, &title, &label)
                    .await
            }
            SessionStrategy::Reuse => {
                let label = project_agent_session_label(routine.agent_id);
                self.find_or_create_project_agent_session(
                    routine.project_id,
                    routine.agent_id,
                    &label,
                )
                .await
            }
            SessionStrategy::PerEntity { entity_key_path } => {
                let entity_key = execution
                    .trigger_payload
                    .as_ref()
                    .and_then(|payload| resolve_json_path(payload, entity_key_path.as_str()))
                    .map(json_value_to_key_string);

                if let Some(ref key) = entity_key {
                    execution.entity_key = Some(key.clone());
                }

                if let Some(ref key) = entity_key
                    && let Some(existing) = self
                        .repos
                        .routine_execution_repo
                        .find_latest_by_entity_key(routine.id, key)
                        .await
                        .map_err(|e| format!("查询 entity session 失败: {e}"))?
                    && let Some(session_id) = existing.session_id
                    && self
                        .session_hub
                        .get_session_meta(&session_id)
                        .await
                        .map_err(|e| format!("读取 session meta 失败: {e}"))?
                        .is_some()
                {
                    return Ok(session_id);
                }

                let suffix = entity_key.as_deref().unwrap_or("unknown");
                let title = format!("Routine: {} [{}]", routine.name, suffix);
                let label = format!("routine:{}:entity:{}", routine.id, suffix);
                self.create_project_owned_session(routine.project_id, &title, &label)
                    .await
            }
        }
    }

    async fn create_project_owned_session(
        &self,
        project_id: Uuid,
        title: &str,
        label: &str,
    ) -> Result<String, String> {
        let meta = self
            .session_hub
            .create_session(title)
            .await
            .map_err(|e| format!("创建 session 失败: {e}"))?;
        let binding = SessionBinding::new(
            project_id,
            meta.id.clone(),
            SessionOwnerType::Project,
            project_id,
            label.to_string(),
        );
        self.repos
            .session_binding_repo
            .create(&binding)
            .await
            .map_err(|e| format!("创建 session binding 失败: {e}"))?;
        self.session_hub
            .mark_owner_bootstrap_pending(&meta.id)
            .await
            .map_err(|e| format!("标记 owner bootstrap 失败: {e}"))?;
        Ok(meta.id)
    }

    async fn find_or_create_project_agent_session(
        &self,
        project_id: Uuid,
        agent_id: Uuid,
        label: &str,
    ) -> Result<String, String> {
        if let Some(binding) = self
            .repos
            .session_binding_repo
            .find_by_owner_and_label(SessionOwnerType::Project, project_id, label)
            .await
            .map_err(|e| format!("查询 session binding 失败: {e}"))?
        {
            let meta = self
                .session_hub
                .get_session_meta(&binding.session_id)
                .await
                .map_err(|e| format!("读取 session meta 失败: {e}"))?;
            if meta.is_some() {
                return Ok(binding.session_id);
            }
            self.repos
                .session_binding_repo
                .delete(binding.id)
                .await
                .map_err(|e| format!("清理失效 session binding 失败: {e}"))?;
        }

        let meta = self
            .session_hub
            .create_session("")
            .await
            .map_err(|e| format!("创建 Project Agent session 失败: {e}"))?;
        let binding = SessionBinding::new(
            project_id,
            meta.id.clone(),
            SessionOwnerType::Project,
            project_id,
            project_agent_session_label(agent_id),
        );
        self.repos
            .session_binding_repo
            .create(&binding)
            .await
            .map_err(|e| format!("创建 Project Agent session binding 失败: {e}"))?;
        self.session_hub
            .mark_owner_bootstrap_pending(&meta.id)
            .await
            .map_err(|e| format!("标记 Project Agent bootstrap 失败: {e}"))?;
        Ok(meta.id)
    }
}

async fn resolve_project_workspace(
    repos: &RepositorySet,
    project: &Project,
) -> Result<Option<Workspace>, String> {
    match project.config.default_workspace_id {
        Some(workspace_id) => {
            let workspace = repos
                .workspace_repo
                .get_by_id(workspace_id)
                .await
                .map_err(|e| format!("查询默认 Workspace 失败: {e}"))?
                .ok_or_else(|| format!("默认 Workspace {workspace_id} 不存在"))?;
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

fn project_agent_session_label(agent_id: Uuid) -> String {
    format!("project_agent:{}", agent_id)
}

fn json_value_to_key_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.trim().to_string(),
        _ => value.to_string(),
    }
}

/// 从 JSON value 中按点分路径取值（如 `"pull_request.number"`）
fn resolve_json_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workspace::{
        WorkspaceBinding, WorkspaceIdentityKind, WorkspaceResolutionPolicy,
    };
    use async_trait::async_trait;
    use serde_json::json;

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

    #[test]
    fn test_resolve_json_path() {
        let data = json!({"a": {"b": {"c": 42}}});
        assert_eq!(resolve_json_path(&data, "a.b.c"), Some(&json!(42)));
        assert_eq!(resolve_json_path(&data, "a.b"), Some(&json!({"c": 42})));
        assert_eq!(resolve_json_path(&data, "x.y"), None);
    }

    #[test]
    fn json_value_to_key_string_prefers_raw_string() {
        assert_eq!(json_value_to_key_string(&json!(" PR-123 ")), "PR-123");
        assert_eq!(json_value_to_key_string(&json!(42)), "42");
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
