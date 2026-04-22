use std::sync::Arc;

use crate::session::SessionHub;
use agentdash_spi::DynAgentTool;
use agentdash_spi::ToolCluster;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{ConnectorError, ExecutionContext};
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::canvas::{BindCanvasDataTool, ListCanvasesTool, PresentCanvasTool, StartCanvasTool};
use crate::companion::tools::{CompanionRequestTool, CompanionRespondTool};
use crate::platform_config::SharedPlatformConfig;
use crate::vfs::inline_persistence::{InlineContentOverlay, InlineContentPersister};
use crate::vfs::relay_service::RelayVfsService;
use crate::vfs::tools::fs::{
    FsApplyPatchTool, FsGlobTool, FsGrepTool, FsReadTool, MountsListTool, SharedRuntimeVfs,
    ShellExecTool,
};
use crate::workflow::tools::advance_node::CompleteLifecycleNodeTool;
use uuid::Uuid;

#[derive(Clone)]
pub struct RelayRuntimeToolProvider {
    service: Arc<RelayVfsService>,
    repos: crate::repository_set::RepositorySet,
    session_hub_handle: SharedSessionHubHandle,
    inline_persister: Option<Arc<dyn InlineContentPersister>>,
    platform_config: SharedPlatformConfig,
}

impl RelayRuntimeToolProvider {
    pub fn new(
        service: Arc<RelayVfsService>,
        repos: crate::repository_set::RepositorySet,
        session_hub_handle: SharedSessionHubHandle,
        inline_persister: Option<Arc<dyn InlineContentPersister>>,
        platform_config: SharedPlatformConfig,
    ) -> Self {
        Self {
            service,
            repos,
            session_hub_handle,
            inline_persister,
            platform_config,
        }
    }
}

#[derive(Clone, Default)]
pub struct SharedSessionHubHandle {
    inner: Arc<RwLock<Option<SessionHub>>>,
}

impl SharedSessionHubHandle {
    pub async fn set(&self, hub: SessionHub) {
        let mut guard = self.inner.write().await;
        *guard = Some(hub);
    }

    pub async fn get(&self) -> Option<SessionHub> {
        self.inner.read().await.clone()
    }
}

#[async_trait]
impl RuntimeToolProvider for RelayRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let vfs = context.vfs.clone().ok_or_else(|| {
            ConnectorError::InvalidConfig("缺少 vfs，无法构建统一访问工具".to_string())
        })?;
        let shared_vfs = SharedRuntimeVfs::new(vfs);

        let overlay: Option<Arc<InlineContentOverlay>> = self
            .inline_persister
            .as_ref()
            .map(|p| Arc::new(InlineContentOverlay::new(p.clone())));

        let identity = context.identity.clone();

        // 合并 session-type 默认簇 与 agent 级 tool_clusters 限制（交集）
        let session_clusters = &context.flow_capabilities.enabled_clusters;
        let effective_clusters = if let Some(ref agent_clusters) =
            context.executor_config.tool_clusters
        {
            let agent_set: std::collections::BTreeSet<ToolCluster> = agent_clusters
                .iter()
                .filter_map(|s| {
                    serde_json::from_value::<ToolCluster>(serde_json::Value::String(s.clone())).ok()
                })
                .collect();
            session_clusters
                .intersection(&agent_set)
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
        } else {
            session_clusters.clone()
        };
        let clusters = &effective_clusters;

        let mut tools: Vec<DynAgentTool> = Vec::new();
        let session_hub = self.session_hub_handle.get().await;
        let excluded = &context.flow_capabilities.excluded_tools;

        // Read 簇：只读文件系统访问
        if clusters.contains(&ToolCluster::Read) {
            tools.push(Arc::new(MountsListTool::new(
                self.service.clone(),
                shared_vfs.clone(),
            )));
            tools.push(Arc::new(FsReadTool::new(
                self.service.clone(),
                shared_vfs.clone(),
                overlay.clone(),
                identity.clone(),
            )));
            tools.push(Arc::new(FsGlobTool::new(
                self.service.clone(),
                shared_vfs.clone(),
                overlay.clone(),
                identity.clone(),
            )));
            tools.push(Arc::new(FsGrepTool::new(
                self.service.clone(),
                shared_vfs.clone(),
                overlay.clone(),
                identity.clone(),
            )));
        }

        // Write 簇：文件写入
        if clusters.contains(&ToolCluster::Write) {
            tools.push(Arc::new(FsApplyPatchTool::new(
                self.service.clone(),
                shared_vfs.clone(),
                overlay.clone(),
                identity,
            )));
        }

        // Execute 簇：命令执行
        if clusters.contains(&ToolCluster::Execute) {
            tools.push(Arc::new(ShellExecTool::new(
                self.service.clone(),
                shared_vfs.clone(),
            )));
        }

        // Workflow 簇：lifecycle node 推进
        if clusters.contains(&ToolCluster::Workflow) {
            tools.push(Arc::new(CompleteLifecycleNodeTool::new(
                self.repos.clone(),
                session_hub.clone(),
                context,
                self.platform_config.clone(),
            )));
        }

        // Collaboration 簇：Companion 协作 + Hook action 解析
        if clusters.contains(&ToolCluster::Collaboration) {
            tools.push(Arc::new(CompanionRequestTool::new(
                self.repos.session_binding_repo.clone(),
                self.repos.agent_repo.clone(),
                self.repos.agent_link_repo.clone(),
                self.session_hub_handle.clone(),
                context,
            )));
            tools.push(Arc::new(CompanionRespondTool::new(
                self.session_hub_handle.clone(),
                context,
            )));
        }

        // Canvas 簇：Canvas 资产工具
        if clusters.contains(&ToolCluster::Canvas) {
            if let Some(project_id) = project_id_from_context(context) {
                tools.push(Arc::new(ListCanvasesTool::new(
                    self.repos.canvas_repo.clone(),
                    project_id,
                )));
                tools.push(Arc::new(StartCanvasTool::new(
                    self.repos.canvas_repo.clone(),
                    project_id,
                    shared_vfs.clone(),
                    self.session_hub_handle.clone(),
                    context
                        .hook_session
                        .as_ref()
                        .map(|session| session.session_id().to_string()),
                )));
                tools.push(Arc::new(BindCanvasDataTool::new(
                    self.repos.canvas_repo.clone(),
                    project_id,
                )));

                if let Some(session_id) = context
                    .hook_session
                    .as_ref()
                    .map(|session| session.session_id().to_string())
                {
                    tools.push(Arc::new(PresentCanvasTool::new(
                        self.repos.canvas_repo.clone(),
                        self.session_hub_handle.clone(),
                        session_id,
                        context.turn_id.clone(),
                        project_id,
                    )));
                }
            } else {
                tracing::warn!("canvas tools 注入失败：无法从 hook session 解析 project_id");
            }
        }

        // 工具级排除：由 CapabilityResolver 从 directive 序列归约而来
        if !excluded.is_empty() {
            tools.retain(|tool| !excluded.contains(tool.name()));
        }

        Ok(tools)
    }
}

fn project_id_from_context(context: &ExecutionContext) -> Option<Uuid> {
    if let Some(hook_session) = context.hook_session.as_ref() {
        let snapshot = hook_session.snapshot();

        // project owner 直接使用 owner_id；story/task owner 使用 owner.project_id。
        for owner in &snapshot.owners {
            use agentdash_domain::session_binding::SessionOwnerType;
            match owner.owner_type {
                SessionOwnerType::Project => {
                    if let Ok(project_id) = Uuid::parse_str(owner.owner_id.as_str()) {
                        return Some(project_id);
                    }
                }
                SessionOwnerType::Story | SessionOwnerType::Task => {
                    if let Some(project_id) = owner.project_id.as_deref()
                        && let Ok(project_id) = Uuid::parse_str(project_id)
                    {
                        return Some(project_id);
                    }
                }
            }
        }
    }

    context
        .vfs
        .as_ref()
        .and_then(|space| space.source_project_id.as_deref())
        .and_then(|project_id| Uuid::parse_str(project_id).ok())
}
