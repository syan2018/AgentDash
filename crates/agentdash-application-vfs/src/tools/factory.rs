use std::sync::Arc;

use agentdash_spi::platform::tool_capability::{CAP_FILE_READ, CAP_FILE_WRITE, CAP_SHELL_EXECUTE};
use agentdash_spi::{CapabilityState, DynAgentTool, ToolCluster};

use crate::VfsMaterializationService;
use crate::inline_persistence::InlineContentOverlay;
use crate::service::VfsService;
use crate::tools::fs::{
    FsApplyPatchTool, FsGlobTool, FsGrepTool, FsReadTool, MountsListTool, SharedRuntimeVfs,
    ShellExecTool, ShellTerminalRegistry,
};

#[derive(Clone)]
pub struct VfsToolFactory {
    service: Arc<VfsService>,
    materialization: Option<Arc<VfsMaterializationService>>,
    shell_output_registry: Option<Arc<agentdash_relay::ShellOutputRegistry>>,
    terminal_registry: Option<Arc<dyn ShellTerminalRegistry>>,
}

impl VfsToolFactory {
    pub fn new(service: Arc<VfsService>) -> Self {
        Self {
            service,
            materialization: None,
            shell_output_registry: None,
            terminal_registry: None,
        }
    }

    pub fn with_materialization(
        mut self,
        materialization: Option<Arc<VfsMaterializationService>>,
    ) -> Self {
        self.materialization = materialization;
        self
    }

    pub fn with_shell_output_registry(
        mut self,
        shell_output_registry: Option<Arc<agentdash_relay::ShellOutputRegistry>>,
    ) -> Self {
        self.shell_output_registry = shell_output_registry;
        self
    }

    pub fn with_terminal_registry(
        mut self,
        terminal_registry: Option<Arc<dyn ShellTerminalRegistry>>,
    ) -> Self {
        self.terminal_registry = terminal_registry;
        self
    }

    pub fn build_tools(&self, input: VfsToolFactoryInput<'_>) -> Vec<DynAgentTool> {
        let clusters = &input.flow.tool.enabled_clusters;
        let mut tools: Vec<DynAgentTool> = Vec::new();

        // Read 簇：只读文件系统访问
        if clusters.contains(&ToolCluster::Read) {
            if input.flow.is_capability_tool_enabled(
                CAP_FILE_READ,
                "mounts_list",
                Some(ToolCluster::Read),
            ) {
                tools.push(Arc::new(MountsListTool::new(
                    self.service.clone(),
                    input.shared_vfs.clone(),
                )));
            }
            if input.flow.is_capability_tool_enabled(
                CAP_FILE_READ,
                "fs_read",
                Some(ToolCluster::Read),
            ) {
                tools.push(Arc::new(FsReadTool::new(
                    self.service.clone(),
                    input.shared_vfs.clone(),
                    input.overlay.clone(),
                    input.identity.clone(),
                )));
            }
            if input.flow.is_capability_tool_enabled(
                CAP_FILE_READ,
                "fs_glob",
                Some(ToolCluster::Read),
            ) {
                tools.push(Arc::new(FsGlobTool::new(
                    self.service.clone(),
                    input.shared_vfs.clone(),
                    input.overlay.clone(),
                    input.identity.clone(),
                )));
            }
            if input.flow.is_capability_tool_enabled(
                CAP_FILE_READ,
                "fs_grep",
                Some(ToolCluster::Read),
            ) {
                tools.push(Arc::new(FsGrepTool::new(
                    self.service.clone(),
                    input.shared_vfs.clone(),
                    input.overlay.clone(),
                    input.identity.clone(),
                )));
            }
        }

        // Write 簇：文件写入
        if clusters.contains(&ToolCluster::Write)
            && input.flow.is_capability_tool_enabled(
                CAP_FILE_WRITE,
                "fs_apply_patch",
                Some(ToolCluster::Write),
            )
        {
            tools.push(Arc::new(FsApplyPatchTool::new(
                self.service.clone(),
                input.shared_vfs.clone(),
                input.overlay.clone(),
                input.identity.clone(),
            )));
        }

        // Execute 簇：命令执行
        if clusters.contains(&ToolCluster::Execute)
            && input.flow.is_capability_tool_enabled(
                CAP_SHELL_EXECUTE,
                "shell_exec",
                Some(ToolCluster::Execute),
            )
        {
            let mut shell_tool = ShellExecTool::new(self.service.clone(), input.shared_vfs.clone())
                .with_materialization_context(
                    self.materialization.clone(),
                    input.session_id.clone(),
                    Some(input.turn_id.clone()),
                    input.overlay.clone(),
                    input.identity.clone(),
                )
                .with_capability_state(input.flow.clone());
            if let Some(ref registry) = self.shell_output_registry {
                shell_tool = shell_tool.with_shell_output_registry(registry.clone());
            }
            if let Some(ref registry) = self.terminal_registry {
                shell_tool = shell_tool.with_terminal_registry(registry.clone());
            }
            tools.push(Arc::new(shell_tool));
        }

        tools
    }
}

pub struct VfsToolFactoryInput<'a> {
    pub shared_vfs: SharedRuntimeVfs,
    pub overlay: Option<Arc<InlineContentOverlay>>,
    pub identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    pub session_id: String,
    pub turn_id: String,
    pub flow: &'a CapabilityState,
}
