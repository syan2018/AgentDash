use std::collections::HashSet;
use std::sync::Arc;

use agent_client_protocol::{
    EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
};
use chrono::Utc;
use uuid::Uuid;

use agentdash_domain::project::Project;
use agentdash_domain::routine::{Routine, RoutineExecution, SessionStrategy};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_domain::workspace::Workspace;
use agentdash_spi::{AgentConfig, AgentConnector};

use crate::capability::{
    CapabilityResolver, CapabilityResolverInput, AgentMcpServerEntry,
};

use crate::vfs::{RelayVfsService, SessionMountTarget};
use crate::canvas::append_visible_canvas_mounts;
use crate::project::context_builder::{
    ProjectContextBuildInput, build_project_context_markdown, build_project_owner_prompt_blocks,
};
use crate::repository_set::RepositorySet;
use crate::runtime_bridge::acp_mcp_servers_to_runtime;
use crate::session::SessionHub;
use crate::session::context::apply_workspace_defaults;
use crate::session::types::{
    PromptSessionRequest, SessionBootstrapAction, SessionPromptLifecycle,
    SessionRepositoryRehydrateMode, UserPromptInput, resolve_session_prompt_lifecycle,
};

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
    mcp_base_url: Option<String>,
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
        mcp_base_url: Option<String>,
    ) -> Self {
        Self {
            repos,
            session_hub,
            vfs_service,
            connector,
            mcp_base_url,
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
        let (preset_mcp_servers, relay_mcp_server_names) = parse_preset_mcp_servers(&merged_config)
            .map_err(|err| format!("Agent `{}` 的 mcp_servers 配置非法: {err}", agent.id))?;

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
        let lifecycle =
            resolve_session_prompt_lifecycle(&meta, has_live_runtime, supports_repository_restore);

        let mut req = PromptSessionRequest::from_user_input(UserPromptInput::from_text(prompt));
        req.user_input.executor_config = Some(agent_context.executor_config.clone());
        req.relay_mcp_server_names
            .extend(agent_context.relay_mcp_server_names.iter().cloned());

        let mut vfs = Some(
            self.vfs_service
                .build_vfs(
                    &agent_context.project,
                    None,
                    agent_context.workspace.as_ref(),
                    SessionMountTarget::Project,
                    Some(agent_context.executor_config.executor.as_str()),
                )
                .map_err(|e| format!("构建 VFS 失败: {e}"))?,
        );
        if let Some(space) = vfs.as_mut() {
            append_visible_canvas_mounts(
                self.repos.canvas_repo.as_ref(),
                agent_context.project.id,
                space,
                &meta.visible_canvas_mount_ids,
            )
            .await
            .map_err(|e| format!("挂载可见 canvas 失败: {e}"))?;
        }

        // ── CapabilityResolver 统一计算工具集 ──
        let agent_mcp_entries: Vec<AgentMcpServerEntry> = agent_context
            .preset_mcp_servers
            .iter()
            .filter_map(|s| {
                let name = match s {
                    McpServer::Http(h) => h.name.clone(),
                    McpServer::Sse(h) => h.name.clone(),
                    McpServer::Stdio(h) => h.name.clone(),
                    _ => return None,
                };
                Some(AgentMcpServerEntry {
                    name,
                    server: s.clone(),
                })
            })
            .collect();

        let cap_input = CapabilityResolverInput {
            owner_type: SessionOwnerType::Project,
            mcp_base_url: self.mcp_base_url.clone(),
            project_id: agent_context.project.id,
            story_id: None,
            task_id: None,
            agent_declared_capabilities: agent_context
                .executor_config
                .tool_clusters
                .as_ref()
                .map(|clusters| {
                    // agent config 中的 tool_clusters 当前仅用于 FlowCapabilities 裁剪，
                    // 未来可扩展为 capability key 声明
                    clusters.clone()
                }),
            has_active_workflow: false,
            workflow_capabilities: vec![],
            agent_mcp_servers: agent_mcp_entries,
        };
        let cap_output = CapabilityResolver::resolve(&cap_input);

        let mut effective_mcp_servers: Vec<McpServer> = cap_output
            .platform_mcp_configs
            .iter()
            .map(|c| c.to_acp_mcp_server())
            .collect();
        effective_mcp_servers.extend(agent_context.preset_mcp_servers.iter().cloned());

        let runtime_vfs = vfs.clone();
        let runtime_mcp_servers = acp_mcp_servers_to_runtime(&effective_mcp_servers);
        let (context_markdown, _) = build_project_context_markdown(ProjectContextBuildInput {
            project: &agent_context.project,
            workspace: agent_context.workspace.as_ref(),
            vfs: runtime_vfs.as_ref(),
            mcp_servers: &runtime_mcp_servers,
            effective_agent_type: Some(agent_context.executor_config.executor.as_str()),
            preset_name: agent_context.preset_name.as_deref(),
            agent_display_name: agent_context.display_name.as_str(),
        });

        let user_prompt_blocks = req
            .user_input
            .prompt_blocks
            .take()
            .ok_or_else(|| "Routine prompt 缺少 prompt_blocks".to_string())?;

        let (system_context, prompt_blocks, bootstrap_action) = match lifecycle {
            SessionPromptLifecycle::OwnerBootstrap => (
                Some(context_markdown.clone()),
                build_project_owner_prompt_blocks(
                    routine.project_id,
                    context_markdown.clone(),
                    user_prompt_blocks,
                ),
                SessionBootstrapAction::OwnerContext,
            ),
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::SystemContext,
            ) => (
                self.session_hub
                    .build_continuation_system_context(session_id, Some(&context_markdown))
                    .await
                    .map_err(|e| format!("构建 continuation context 失败: {e}"))?,
                user_prompt_blocks,
                SessionBootstrapAction::None,
            ),
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::ExecutorState,
            ) => (
                Some(context_markdown.clone()),
                user_prompt_blocks,
                SessionBootstrapAction::None,
            ),
            SessionPromptLifecycle::Plain => {
                (None, user_prompt_blocks, SessionBootstrapAction::None)
            }
        };

        req.user_input.prompt_blocks = Some(prompt_blocks);
        req.system_context = system_context;
        req.bootstrap_action = bootstrap_action;

        apply_workspace_defaults(
            &mut req.user_input.working_dir,
            &mut req.vfs,
            agent_context.workspace.as_ref(),
        );
        if req.vfs.is_none() {
            req.vfs = vfs;
        }
        req.mcp_servers = effective_mcp_servers;
        req.flow_capabilities = Some(cap_output.flow_capabilities);

        Ok(req)
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

fn parse_preset_mcp_servers(
    config: &serde_json::Value,
) -> Result<(Vec<McpServer>, HashSet<String>), String> {
    let raw_list = match config.get("mcp_servers").and_then(|v| v.as_array()) {
        Some(list) => list,
        None => return Ok((vec![], HashSet::new())),
    };

    let mut mcp_servers = Vec::new();
    let mut relay_names = HashSet::new();

    for (index, entry) in raw_list.iter().enumerate() {
        let obj = entry
            .as_object()
            .ok_or_else(|| format!("mcp_servers[{index}] 必须是对象"))?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("mcp_servers[{index}].name 缺失或为空"))?
            .to_string();

        let effective_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("mcp_servers[{index}].type 缺失或不是字符串"))?;

        let is_relay = obj
            .get("relay")
            .and_then(|v| v.as_bool())
            .unwrap_or(effective_type == "stdio");
        if is_relay {
            relay_names.insert(name.clone());
        }

        match effective_type {
            "http" | "sse" => {
                let url = obj
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("mcp_servers[{index}].url 缺失或为空"))?
                    .to_string();
                let headers = match obj.get("headers") {
                    Some(value) => value
                        .as_array()
                        .ok_or_else(|| format!("mcp_servers[{index}].headers 必须是数组"))?
                        .iter()
                        .enumerate()
                        .map(|(header_index, header)| {
                            let header_obj = header.as_object().ok_or_else(|| {
                                format!("mcp_servers[{index}].headers[{header_index}] 必须是对象")
                            })?;
                            let header_name = header_obj
                                .get("name")
                                .and_then(|v| v.as_str())
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| {
                                    format!(
                                        "mcp_servers[{index}].headers[{header_index}].name 缺失或为空"
                                    )
                                })?;
                            let header_value = header_obj
                                .get("value")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| {
                                    format!(
                                        "mcp_servers[{index}].headers[{header_index}].value 缺失或不是字符串"
                                    )
                                })?;
                            Ok::<HttpHeader, String>(HttpHeader::new(
                                header_name.to_string(),
                                header_value.to_string(),
                            ))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    None => Vec::new(),
                };

                if effective_type == "http" {
                    mcp_servers.push(McpServer::Http(
                        McpServerHttp::new(name, url).headers(headers),
                    ));
                } else {
                    mcp_servers.push(McpServer::Sse(
                        McpServerSse::new(name, url).headers(headers),
                    ));
                }
            }
            "stdio" => {
                let command = obj
                    .get("command")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("mcp_servers[{index}].command 缺失或为空"))?
                    .to_string();
                let args = match obj.get("args") {
                    Some(value) => value
                        .as_array()
                        .ok_or_else(|| format!("mcp_servers[{index}].args 必须是数组"))?
                        .iter()
                        .enumerate()
                        .map(|(arg_index, arg)| {
                            arg.as_str().map(String::from).ok_or_else(|| {
                                format!("mcp_servers[{index}].args[{arg_index}] 必须是字符串")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    None => Vec::new(),
                };
                let env = match obj.get("env") {
                    Some(value) => value
                        .as_array()
                        .ok_or_else(|| format!("mcp_servers[{index}].env 必须是数组"))?
                        .iter()
                        .enumerate()
                        .map(|(env_index, entry)| {
                            let env_obj = entry.as_object().ok_or_else(|| {
                                format!("mcp_servers[{index}].env[{env_index}] 必须是对象")
                            })?;
                            let env_name = env_obj
                                .get("name")
                                .and_then(|v| v.as_str())
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| {
                                    format!(
                                        "mcp_servers[{index}].env[{env_index}].name 缺失或为空"
                                    )
                                })?;
                            let env_value = env_obj
                                .get("value")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| {
                                    format!(
                                        "mcp_servers[{index}].env[{env_index}].value 缺失或不是字符串"
                                    )
                                })?;
                            Ok::<EnvVariable, String>(EnvVariable::new(
                                env_name.to_string(),
                                env_value.to_string(),
                            ))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    None => Vec::new(),
                };
                mcp_servers.push(McpServer::Stdio(
                    McpServerStdio::new(name, command).args(args).env(env),
                ));
            }
            other => {
                return Err(format!("mcp_servers[{index}].type 非法，不支持 `{other}`"));
            }
        }
    }

    Ok((mcp_servers, relay_names))
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
