//! 统一声明式 Session 构造器 — 所有 owner type 共享 VFS + MCP + Context 构造仪式。
//!
//! API handler 只需声明 "我是谁"（SessionOwnerVariant）和少量配置参数，
//! builder 自动完成 VFS 构建、canvas 挂载、CapabilityResolver 调用和 context markdown 生成。

use std::collections::HashSet;

use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::project::Project;
use agentdash_domain::session_binding::SessionOwnerCtx;
use agentdash_domain::story::Story;
use agentdash_domain::workspace::Workspace;
use agentdash_spi::{FlowCapabilities, Vfs};

use crate::capability::{AgentMcpServerEntry, CapabilityResolver, CapabilityResolverInput};
use crate::canvas::append_visible_canvas_mounts;
use crate::platform_config::PlatformConfig;
use crate::context::resolve_workspace_declared_sources;
use crate::project::context_builder::{ProjectContextBuildInput, build_project_context_markdown};
use crate::runtime::RuntimeMcpServer;
use crate::story::context_builder::{StoryContextBuildInput, build_story_context_markdown};
use crate::vfs::{RelayVfsService, SessionMountTarget};
use crate::workspace::BackendAvailability;

/// builder 所需的基础设施引用（不可变借用，跨调用共享）。
pub struct SessionPlanServices<'a> {
    pub vfs_service: &'a RelayVfsService,
    pub canvas_repo: &'a dyn CanvasRepository,
    pub availability: &'a dyn BackendAvailability,
}

/// 区分 session owner 类型及其携带的上下文数据。
pub enum SessionOwnerVariant<'a> {
    Story {
        story: &'a Story,
        project: &'a Project,
        workspace: Option<&'a Workspace>,
    },
    Project {
        project: &'a Project,
        workspace: Option<&'a Workspace>,
        agent_display_name: String,
        preset_name: Option<String>,
        /// agent preset 级 MCP servers（非平台内置，由 project agent 配置携带）
        preset_mcp_servers: Vec<agent_client_protocol::McpServer>,
        /// agent preset 级 relay MCP server 名称（透传到 PromptSessionRequest）
        relay_mcp_server_names: HashSet<String>,
    },
}

/// builder 的声明式输入。
pub struct SessionPlanInput<'a> {
    pub owner: SessionOwnerVariant<'a>,
    /// 已解析的 executor config（agent type + model 等）
    pub effective_agent_type: Option<&'a str>,
    /// 进程级平台配置
    pub platform_config: &'a PlatformConfig,
    /// session meta 中记录的可见 canvas mount_id 列表
    pub visible_canvas_mount_ids: &'a [String],
    /// Workflow 上下文：是否活跃 + 显式目标能力集合。
    /// 调用方用 [`crate::capability::resolve_session_workflow_context`] 装配，
    /// 无绑定时传 [`crate::capability::SessionWorkflowContext::NONE`]。
    pub workflow_ctx: crate::capability::SessionWorkflowContext,
    /// agent config 中注册的 MCP servers（用于兼容旧 inline 声明链路,
    /// 新模型应优先用 `available_presets`）。
    pub agent_mcp_servers: Vec<AgentMcpServerEntry>,
    /// project 级 MCP Preset 预展开字典 — 供 resolver 解析 `mcp:<preset_name>` 能力。
    /// 调用方从 `McpPresetRepository::list_by_project` 查询后展开传入。
    pub available_presets: crate::capability::AvailableMcpPresets,
    /// 请求已携带的 MCP servers（前端透传）
    pub request_mcp_servers: Vec<agent_client_protocol::McpServer>,
    /// 请求已携带的 VFS（不为 None 时跳过 VFS 构建）
    pub existing_vfs: Option<Vfs>,
    /// agent config 中显式声明的 capability key 列表
    pub agent_declared_capabilities: Option<Vec<String>>,
}

/// builder 的完整产出 — 下游 lifecycle handler 和 finalize 直接消费。
pub struct SessionPlanOutput {
    pub flow_capabilities: FlowCapabilities,
    pub effective_mcp_servers: Vec<agent_client_protocol::McpServer>,
    pub vfs: Option<Vfs>,
    pub context_markdown: String,
    /// 仅 Project owner 路径携带（透传到 PromptSessionRequest）
    pub relay_mcp_server_names: HashSet<String>,
    /// 已解析的 capability string key 集合（供 hook runtime 初始化 capabilities 追踪）
    pub effective_capability_keys: std::collections::BTreeSet<String>,
}

pub struct SessionPlanBuilder;

impl SessionPlanBuilder {
    pub async fn build(
        svc: &SessionPlanServices<'_>,
        input: SessionPlanInput<'_>,
    ) -> Result<SessionPlanOutput, String> {
        let (project_id, owner_ctx) = match &input.owner {
            SessionOwnerVariant::Story {
                story, project, ..
            } => (
                project.id,
                SessionOwnerCtx::Story {
                    project_id: project.id,
                    story_id: story.id,
                },
            ),
            SessionOwnerVariant::Project { project, .. } => (
                project.id,
                SessionOwnerCtx::Project {
                    project_id: project.id,
                },
            ),
        };

        // ── 1. VFS 构建 + canvas 挂载 ──
        let mut vfs = match input.existing_vfs {
            Some(vfs) => Some(vfs),
            None => {
                let built = match &input.owner {
                    SessionOwnerVariant::Story {
                        story,
                        project,
                        workspace,
                    } => svc.vfs_service.build_vfs(
                        project,
                        Some(*story),
                        *workspace,
                        SessionMountTarget::Story,
                        input.effective_agent_type,
                    )?,
                    SessionOwnerVariant::Project {
                        project, workspace, ..
                    } => svc.vfs_service.build_vfs(
                        project,
                        None,
                        *workspace,
                        SessionMountTarget::Project,
                        input.effective_agent_type,
                    )?,
                };
                Some(built)
            }
        };
        if let Some(space) = vfs.as_mut() {
            append_visible_canvas_mounts(
                svc.canvas_repo,
                project_id,
                space,
                input.visible_canvas_mount_ids,
            )
            .await
            .map_err(|e| e.to_string())?;
        }

        // ── 2. CapabilityResolver ──
        let cap_output = CapabilityResolver::resolve(
            &CapabilityResolverInput {
                owner_ctx,
                agent_declared_capabilities: input.agent_declared_capabilities,
                workflow_ctx: input.workflow_ctx,
                agent_mcp_servers: input.agent_mcp_servers,
                available_presets: input.available_presets,
                companion_slice_mode: None,
            },
            input.platform_config,
        );

        // ── 3. MCP server 列表汇总 ──
        let mut effective_mcp_servers = input.request_mcp_servers;
        for config in &cap_output.platform_mcp_configs {
            effective_mcp_servers.push(config.to_acp_mcp_server());
        }
        effective_mcp_servers.extend(cap_output.custom_mcp_servers.iter().cloned());
        let mut relay_mcp_server_names = HashSet::new();
        if let SessionOwnerVariant::Project {
            preset_mcp_servers,
            relay_mcp_server_names: preset_relay_names,
            ..
        } = &input.owner
        {
            effective_mcp_servers.extend(preset_mcp_servers.iter().cloned());
            relay_mcp_server_names.extend(preset_relay_names.iter().cloned());
        }

        // ── 4. Context markdown 生成 ──
        let runtime_mcp_servers = acp_mcp_servers_to_runtime(&effective_mcp_servers);
        let runtime_vfs = vfs.clone();

        let context_markdown = match &input.owner {
            SessionOwnerVariant::Story {
                story,
                project,
                workspace,
            } => {
                let resolved_workspace_sources = resolve_workspace_declared_sources(
                    svc.availability,
                    svc.vfs_service,
                    &story.context.source_refs,
                    *workspace,
                    60,
                )
                .await?;

                let (md, _) = build_story_context_markdown(StoryContextBuildInput {
                    story,
                    project,
                    workspace: *workspace,
                    vfs: runtime_vfs.as_ref(),
                    mcp_servers: &runtime_mcp_servers,
                    effective_agent_type: input.effective_agent_type,
                    workspace_source_fragments: resolved_workspace_sources.fragments,
                    workspace_source_warnings: resolved_workspace_sources.warnings,
                });
                md
            }
            SessionOwnerVariant::Project {
                project,
                workspace,
                agent_display_name,
                preset_name,
                ..
            } => {
                let (md, _) = build_project_context_markdown(ProjectContextBuildInput {
                    project,
                    workspace: workspace.as_deref(),
                    vfs: runtime_vfs.as_ref(),
                    mcp_servers: &runtime_mcp_servers,
                    effective_agent_type: input.effective_agent_type,
                    preset_name: preset_name.as_deref(),
                    agent_display_name,
                });
                md
            }
        };

        let effective_capability_keys = cap_output
            .effective_capabilities
            .iter()
            .map(|c| c.key().to_string())
            .collect();

        Ok(SessionPlanOutput {
            flow_capabilities: cap_output.flow_capabilities,
            effective_mcp_servers,
            vfs,
            context_markdown,
            relay_mcp_server_names,
            effective_capability_keys,
        })
    }
}

/// ACP McpServer → RuntimeMcpServer 转换（复用既有逻辑模式）
fn acp_mcp_servers_to_runtime(
    servers: &[agent_client_protocol::McpServer],
) -> Vec<RuntimeMcpServer> {
    servers
        .iter()
        .filter_map(|server| match server {
            agent_client_protocol::McpServer::Http(http) => Some(RuntimeMcpServer::Http {
                name: http.name.clone(),
                url: http.url.clone(),
            }),
            agent_client_protocol::McpServer::Sse(sse) => Some(RuntimeMcpServer::Http {
                name: sse.name.clone(),
                url: sse.url.clone(),
            }),
            _ => None,
        })
        .collect()
}
