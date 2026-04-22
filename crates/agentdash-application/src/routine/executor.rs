use std::collections::HashSet;
use std::sync::Arc;

use agent_client_protocol::McpServer;
use chrono::Utc;
use uuid::Uuid;

use agentdash_domain::project::Project;
use agentdash_domain::routine::{Routine, RoutineExecution, SessionStrategy};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_domain::workspace::Workspace;
use agentdash_spi::{AgentConfig, AgentConnector};

use crate::context::ContextContributorRegistry;
use crate::repository_set::RepositorySet;
use crate::session::SessionHub;
use crate::session::types::{
    PromptSessionRequest, SessionPromptLifecycle, SessionRepositoryRehydrateMode, UserPromptInput,
    resolve_session_prompt_lifecycle,
};
use crate::session::{
    AgentLevelMcp, OwnerBootstrapSpec, OwnerPromptLifecycle, OwnerScope, SessionRequestAssembler,
    finalize_request,
};
use crate::vfs::RelayVfsService;
use crate::workspace::BackendAvailability;

use super::template::render_prompt_template;

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
    vfs_service: Arc<RelayVfsService>,
    connector: Arc<dyn AgentConnector>,
    platform_config: crate::platform_config::SharedPlatformConfig,
    contributor_registry: Arc<ContextContributorRegistry>,
    availability: Arc<dyn BackendAvailability>,
}

struct RoutineAgentContext {
    project: Project,
    workspace: Option<Workspace>,
    executor_config: AgentConfig,
    display_name: String,
    preset_name: Option<String>,
    preset_mcp_servers: Vec<McpServer>,
    relay_mcp_server_names: HashSet<String>,
}

impl RoutineExecutor {
    pub fn new(
        repos: RepositorySet,
        session_hub: SessionHub,
        vfs_service: Arc<RelayVfsService>,
        connector: Arc<dyn AgentConnector>,
        platform_config: crate::platform_config::SharedPlatformConfig,
        contributor_registry: Arc<ContextContributorRegistry>,
        availability: Arc<dyn BackendAvailability>,
    ) -> Self {
        Self {
            repos,
            session_hub,
            vfs_service,
            connector,
            platform_config,
            contributor_registry,
            availability,
        }
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

        let agent_context = self
            .load_agent_context(&routine)
            .await
            .map_err(|err| format!("加载 Routine Agent 配置失败: {err}"))?;

        match self
            .execute_with_session(&routine, &agent_context, &rendered, &mut execution)
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
        agent_context: &RoutineAgentContext,
        prompt: &str,
        execution: &mut RoutineExecution,
    ) -> Result<(), String> {
        let session_id = self
            .resolve_session_id(routine, execution)
            .await
            .map_err(|err| format!("解析 Routine session 失败: {err}"))?;
        let req = self
            .build_project_agent_prompt_request(&session_id, routine, agent_context, prompt)
            .await?;

        execution.mark_running(&session_id, prompt.to_string());
        let _ = self.repos.routine_execution_repo.update(execution).await;

        let _turn_id = self
            .session_hub
            .start_prompt(&session_id, req)
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

        let merged_config = link.merged_config(&agent.base_config);
        let executor_config = build_agent_config_from_merged(&agent.agent_type, &merged_config);
        let display_name = merged_config
            .get("display_name")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(agent.name.as_str())
            .to_string();
        let (preset_mcp_servers, relay_mcp_server_names) =
            crate::mcp_preset::resolve_config_mcp_preset_refs(
                self.repos.mcp_preset_repo.as_ref(),
                project.id,
                &merged_config,
            )
            .await
            .map_err(|err| format!("Agent `{}` 的 mcp_preset_keys 配置非法: {err}", agent.id))?;

        Ok(RoutineAgentContext {
            project,
            workspace,
            executor_config,
            display_name,
            preset_name: Some(agent.name.clone()),
            preset_mcp_servers,
            relay_mcp_server_names,
        })
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

    async fn build_project_agent_prompt_request(
        &self,
        session_id: &str,
        routine: &Routine,
        agent_context: &RoutineAgentContext,
        prompt: &str,
    ) -> Result<PromptSessionRequest, String> {
        let meta = self
            .session_hub
            .get_session_meta(session_id)
            .await
            .map_err(|e| format!("读取 session meta 失败: {e}"))?
            .ok_or_else(|| format!("session {session_id} 不存在"))?;
        let has_live_runtime = self.session_hub.has_live_runtime(session_id).await;
        let supports_repository_restore = self
            .connector
            .supports_repository_restore(agent_context.executor_config.executor.as_str());
        let kind =
            resolve_session_prompt_lifecycle(&meta, has_live_runtime, supports_repository_restore);

        // Routine 的 prompt 是纯文本模板渲染结果 —— 包成单 text block 作为 user_prompt_blocks
        let user_prompt_blocks = vec![serde_json::json!({
            "type": "text",
            "text": prompt,
        })];

        // RepositoryRehydrate(SystemContext) 需要预查 continuation system context
        let lifecycle = match kind {
            SessionPromptLifecycle::OwnerBootstrap => OwnerPromptLifecycle::OwnerBootstrap,
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::SystemContext,
            ) => {
                let ctx = self
                    .session_hub
                    .build_continuation_system_context(session_id, None)
                    .await
                    .map_err(|e| format!("构建 continuation context 失败: {e}"))?;
                OwnerPromptLifecycle::RepositoryRehydrate {
                    prebuilt_continuation_system_context: ctx,
                    include_markdown_as_system_context: false,
                }
            }
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::ExecutorState,
            ) => OwnerPromptLifecycle::RepositoryRehydrate {
                prebuilt_continuation_system_context: None,
                include_markdown_as_system_context: true,
            },
            SessionPromptLifecycle::Plain => OwnerPromptLifecycle::Plain,
        };

        let assembler = SessionRequestAssembler::new(
            self.vfs_service.as_ref(),
            self.repos.canvas_repo.as_ref(),
            self.availability.as_ref(),
            &self.repos,
            &self.platform_config,
            self.contributor_registry.as_ref(),
        );

        let agent_declared_capabilities = agent_context
            .executor_config
            .tool_clusters
            .as_ref()
            .cloned();

        let base = PromptSessionRequest::from_user_input(UserPromptInput::from_text(prompt));
        let prepared = assembler
            .compose_owner_bootstrap(OwnerBootstrapSpec {
                owner: OwnerScope::Project {
                    project: &agent_context.project,
                    workspace: agent_context.workspace.as_ref(),
                    agent_id: Some(routine.agent_id),
                    agent_display_name: agent_context.display_name.clone(),
                    preset_name: agent_context.preset_name.clone(),
                },
                executor_config: agent_context.executor_config.clone(),
                user_prompt_blocks,
                agent_mcp: AgentLevelMcp {
                    preset_mcp_servers: agent_context.preset_mcp_servers.clone(),
                    relay_mcp_server_names: agent_context.relay_mcp_server_names.clone(),
                },
                request_mcp_servers: Vec::new(),
                existing_vfs: None,
                visible_canvas_mount_ids: meta.visible_canvas_mount_ids.clone(),
                agent_declared_capabilities,
                lifecycle,
            })
            .await?;

        Ok(finalize_request(base, prepared))
    }
}

async fn resolve_project_workspace(
    repos: &RepositorySet,
    project: &Project,
) -> Result<Option<Workspace>, String> {
    match project.config.default_workspace_id {
        Some(workspace_id) => repos
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(|e| format!("查询默认 Workspace 失败: {e}")),
        None => Ok(None),
    }
}

fn build_agent_config_from_merged(agent_type: &str, config: &serde_json::Value) -> AgentConfig {
    let mut executor_config = AgentConfig::new(agent_type.to_string());
    if let Some(value) = config.get("provider_id").and_then(|v| v.as_str()) {
        executor_config.provider_id = Some(value.to_string());
    }
    if let Some(value) = config.get("model_id").and_then(|v| v.as_str()) {
        executor_config.model_id = Some(value.to_string());
    }
    if let Some(value) = config.get("agent_id").and_then(|v| v.as_str()) {
        executor_config.agent_id = Some(value.to_string());
    }
    if let Some(value) = config.get("permission_policy").and_then(|v| v.as_str()) {
        executor_config.permission_policy = Some(value.to_string());
    }
    if let Some(value) = config
        .get("thinking_level")
        .and_then(|v| serde_json::from_value::<agentdash_spi::ThinkingLevel>(v.clone()).ok())
    {
        executor_config.thinking_level = Some(value);
    }
    if let Some(arr) = config.get("tool_clusters").and_then(|v| v.as_array()) {
        let clusters = arr
            .iter()
            .filter_map(|value| value.as_str().map(String::from))
            .collect::<Vec<_>>();
        if !clusters.is_empty() {
            executor_config.tool_clusters = Some(clusters);
        }
    }
    if let Some(value) = config.get("system_prompt").and_then(|v| v.as_str()) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            executor_config.system_prompt = Some(trimmed.to_string());
        }
    }
    if let Some(value) = config
        .get("system_prompt_mode")
        .and_then(|v| serde_json::from_value::<agentdash_spi::SystemPromptMode>(v.clone()).ok())
    {
        executor_config.system_prompt_mode = Some(value);
    }
    executor_config
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
    use serde_json::json;

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
}
