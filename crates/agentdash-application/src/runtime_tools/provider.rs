use std::sync::Arc;

use agentdash_application_runtime_gateway::RuntimeGateway;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{ConnectorError, DynAgentTool, ExecutionContext};
use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::agent_run::AgentRunRuntimeSurfaceUpdateService;
use crate::session::{
    SessionControlService, SessionCoreService, SessionEventingService, SessionHookService,
    SessionLaunchService, SessionRuntimeTransitionService,
};
use crate::vfs::tools::fs::SharedRuntimeVfs;

/// `RuntimeGateway` 的延迟注入句柄。
///
/// gateway 在 app_state 装配序里晚于 session runtime tool composer 构造
/// （gateway 依赖 session_mcp_access，后者又依赖 runtime tool surface），
/// 因此由 app_state 在 gateway 建好后回填。
#[derive(Clone, Default)]
pub struct SharedRuntimeGatewayHandle {
    inner: Arc<RwLock<Option<Arc<RuntimeGateway>>>>,
}

impl SharedRuntimeGatewayHandle {
    pub async fn set(&self, gateway: Arc<RuntimeGateway>) {
        *self.inner.write().await = Some(gateway);
    }

    pub async fn get(&self) -> Option<Arc<RuntimeGateway>> {
        self.inner.read().await.clone()
    }
}

#[derive(Clone)]
pub struct SessionToolServices {
    pub core: SessionCoreService,
    pub eventing: SessionEventingService,
    pub control: SessionControlService,
    pub launch: SessionLaunchService,
    pub hooks: SessionHookService,
    pub runtime_transition: SessionRuntimeTransitionService,
    pub runtime_surface_update: AgentRunRuntimeSurfaceUpdateService,
}

#[derive(Clone, Default)]
pub struct SharedSessionToolServicesHandle {
    inner: Arc<RwLock<Option<SessionToolServices>>>,
}

impl SharedSessionToolServicesHandle {
    pub async fn set(&self, services: SessionToolServices) {
        let mut guard = self.inner.write().await;
        *guard = Some(services);
    }

    pub async fn get(&self) -> Option<SessionToolServices> {
        self.inner.read().await.clone()
    }
}

#[derive(Clone, Default)]
pub struct SessionRuntimeToolComposer {
    providers: Vec<Arc<dyn RuntimeToolProvider>>,
}

impl SessionRuntimeToolComposer {
    pub fn new(providers: Vec<Arc<dyn RuntimeToolProvider>>) -> Self {
        Self { providers }
    }

    pub fn with_provider(mut self, provider: Arc<dyn RuntimeToolProvider>) -> Self {
        self.providers.push(provider);
        self
    }
}

#[async_trait]
impl RuntimeToolProvider for SessionRuntimeToolComposer {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let mut tools = Vec::new();
        for provider in &self.providers {
            tools.extend(provider.build_tools(context).await?);
        }
        Ok(tools)
    }
}

pub(crate) fn shared_runtime_vfs_from_context(
    context: &ExecutionContext,
) -> Result<SharedRuntimeVfs, ConnectorError> {
    let vfs = context.session.vfs.clone().ok_or_else(|| {
        ConnectorError::InvalidConfig("缺少 vfs，无法构建统一访问工具".to_string())
    })?;
    Ok(SharedRuntimeVfs::new(vfs))
}

pub(crate) fn runtime_session_id_from_context(context: &ExecutionContext) -> String {
    context
        .turn
        .hook_runtime
        .as_ref()
        .map(|session| session.session_id().to_string())
        .unwrap_or_else(|| context.session.turn_id.clone())
}

pub(crate) fn project_id_from_context(context: &ExecutionContext) -> Option<Uuid> {
    if let Some(hook_runtime) = context.turn.hook_runtime.as_ref() {
        let snapshot = hook_runtime.snapshot();

        if let Some(run_context) = &snapshot.run_context {
            return Some(run_context.project_id);
        }
    }

    context
        .session
        .vfs
        .as_ref()
        .and_then(|space| space.source_project_id.as_deref())
        .and_then(|project_id| Uuid::parse_str(project_id).ok())
}
