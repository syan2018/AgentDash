use std::collections::HashMap;

use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::common::AgentConfig;
use agentdash_domain::workspace::Workspace;
use agentdash_spi::{CapabilityState, SessionContextBundle, Vfs};
use uuid::Uuid;

use crate::canvas::append_visible_canvas_mounts;
use crate::capability::CapabilityResolver;
use crate::companion::tools::CompanionSliceMode;
use crate::session::capability_state::compose_vfs_with_overlay_and_directives;
#[cfg(test)]
use crate::session::construction::RuntimeContextInspectionPlan;
#[cfg(test)]
use crate::session::context::apply_workspace_defaults;
use crate::session::types::UserPromptInput;
use crate::vfs::build_lifecycle_mount_with_ports;

/// 把 `SessionAssemblyBuilder` 的累积声明合并进 construction provider handoff。
///
/// ## 合并语义（2026-04-30 对称化后）
///
/// | 字段 | 策略 |
/// |---|---|
/// | `prompt_blocks` | `Option`：prepared 非空覆盖；否则保留 base |
/// | `executor_config` | `Option`：prepared 非空覆盖；否则保留 base |
/// | `context_bundle` / `capability_state` | 整体替换为 prepared 值 |
/// | `vfs` | prepared 非空覆盖；否则 `apply_workspace_defaults` 按需从 workspace 回填 |
/// | `mcp_servers` | **整体替换** 为 prepared 值（compose 内部已汇总 request + platform + custom + preset） |
/// | `env` | prepared 非空（`!is_empty()`）时整体替换；否则保留 base 的 env |
///
/// **注**：`mcp_servers` 已迁移为 `Vec<SessionMcpServer>` 内部类型，relay 标记
/// 内嵌于每个 server 实例，不再作为独立字段传递。
#[cfg(test)]
pub(crate) fn apply_session_assembly(
    mut plan: RuntimeContextInspectionPlan,
    prepared: SessionAssemblyBuilder,
) -> RuntimeContextInspectionPlan {
    if let Some(blocks) = prepared.prompt_blocks {
        plan.prompt.prompt_blocks = Some(blocks);
    }
    if let Some(cfg) = prepared.executor_config {
        plan.execution_profile.executor_config = Some(cfg);
    }
    plan.context.bundle = prepared.context_bundle;
    plan.context.bundle_id = plan.context.bundle.as_ref().map(|bundle| bundle.bundle_id);
    plan.context.bootstrap_fragment_count = plan
        .context
        .bundle
        .as_ref()
        .map(|bundle| bundle.bootstrap_fragments.len())
        .unwrap_or_default();

    apply_workspace_defaults(&mut plan.surface.vfs, prepared.workspace_defaults.as_ref());
    // vfs 覆盖规则：prepared 非空则覆盖，否则保留（含 workspace_defaults 回填结果）。
    // 语义等价于旧的三重分支，但表达更直接；compose 产出的 workspace/canvas/lifecycle
    // mount 组合会覆盖前端透传的 vfs，是刻意为之。
    let active_vfs = prepared.vfs.or_else(|| plan.surface.vfs.clone());
    plan.projections.mcp_servers = prepared.mcp_servers;
    plan.projections.capability_state = prepared.capability_state;
    if let Some(vfs) = active_vfs {
        plan.set_active_vfs(vfs);
    } else {
        plan.sync_vfs_projection_from_capability();
    }
    if !prepared.env.is_empty() {
        plan.prompt.environment_variables = prepared.env;
    }
    plan
}

/// 声明式 session 装配 builder。
///
/// 将 session 启动拆为 6 个正交关注点（VFS / 能力 / MCP / 系统上下文 / Prompt / 工作流），
/// 每个关注点通过独立的 `with_*` 方法注入，最终投影到分组后的 launch request。
///
/// ## 设计原则
///
/// - **每个层独立**：`with_*` 方法只写入自己关注的字段，不覆盖其他层
/// - **追加友好**：MCP / relay 等集合字段支持多次 `append`
/// - **复合便利**：`apply_companion_slice` / `apply_lifecycle_activation` 封装常见组合
/// - **新组合无需新函数**：companion + workflow 只需叠加对应层
#[derive(Clone, Default)]
pub(super) struct SessionAssemblyBuilder {
    // ── VFS 层 ──
    pub(super) vfs: Option<Vfs>,

    // ── 能力层 ──
    pub(super) capability_state: Option<CapabilityState>,

    // ── MCP 层 ──
    pub(super) mcp_servers: Vec<agentdash_spi::SessionMcpServer>,

    // ── 系统上下文层 ──
    pub(super) context_bundle: Option<SessionContextBundle>,

    // ── Prompt 层 ──
    pub(super) prompt_blocks: Option<Vec<serde_json::Value>>,
    pub(super) executor_config: Option<AgentConfig>,

    // ── 元信息层 ──
    pub(super) workspace_defaults: Option<Workspace>,

    // ── 用户输入侧 ──
    pub(super) env: HashMap<String, String>,
}

#[allow(dead_code)]
impl SessionAssemblyBuilder {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// 直接设置完整 VFS（owner 构建 / lifecycle 激活产出等场景）。
    pub(super) fn with_vfs(mut self, vfs: Vfs) -> Self {
        self.vfs = Some(vfs);
        self
    }

    /// 从父 session 切片生成 companion VFS。
    pub(super) fn with_companion_vfs(
        mut self,
        parent_vfs: Option<&Vfs>,
        mode: CompanionSliceMode,
    ) -> Self {
        use crate::companion::tools::build_companion_execution_slice;
        let slice = build_companion_execution_slice(parent_vfs, &[], mode);
        self.vfs = slice.vfs;
        self
    }

    /// 在已有 VFS 上追加 lifecycle mount（story step activation 场景）。
    pub(super) fn append_lifecycle_mount(
        mut self,
        run_id: Uuid,
        graph_instance_id: Uuid,
        lifecycle_key: &str,
        writable_port_keys: &[String],
    ) -> Self {
        let lifecycle_mount = build_lifecycle_mount_with_ports(
            run_id,
            graph_instance_id,
            lifecycle_key,
            writable_port_keys,
        );
        let mut overlay = Vfs::default();
        overlay.mounts.push(lifecycle_mount);
        self.vfs = Some(compose_vfs_with_overlay_and_directives(
            self.vfs.as_ref(),
            &overlay,
            &[],
        ));
        self
    }

    /// 在已有 VFS 上追加 canvas mount。
    pub(super) async fn append_canvas_mounts(
        mut self,
        canvas_repo: &dyn CanvasRepository,
        project_id: Uuid,
        mount_ids: &[String],
    ) -> Result<Self, String> {
        if let Some(space) = self.vfs.as_mut() {
            append_visible_canvas_mounts(canvas_repo, project_id, space, mount_ids)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(self)
    }

    /// 设置已解析的能力输出（由外部 CapabilityResolver 产出）。
    pub(super) fn with_resolved_capabilities(mut self, capability_state: CapabilityState) -> Self {
        self.capability_state = Some(capability_state);
        self
    }

    /// 使用 companion 专属能力裁剪。
    pub(super) fn with_companion_capabilities(mut self, mode: CompanionSliceMode) -> Self {
        let flow_caps = CapabilityResolver::resolve_companion_caps(mode);
        self.capability_state = Some(flow_caps);
        self
    }

    /// 设置 MCP server 列表（覆盖）。
    pub(super) fn with_mcp_servers(
        mut self,
        servers: Vec<agentdash_spi::SessionMcpServer>,
    ) -> Self {
        self.mcp_servers = servers;
        self
    }

    /// 追加 MCP server 到列表。
    pub(super) fn append_mcp_servers(
        mut self,
        servers: impl IntoIterator<Item = agentdash_spi::SessionMcpServer>,
    ) -> Self {
        self.mcp_servers.extend(servers);
        self
    }

    /// 设置结构化上下文 Bundle —— 所有 connector 的主数据源。
    pub(super) fn with_context_bundle(mut self, bundle: SessionContextBundle) -> Self {
        self.context_bundle = Some(bundle);
        self
    }

    /// 可选设置 Bundle；为 `None` 时不覆盖已有值（用于 continuation 路径按条件注入）。
    pub(super) fn with_optional_context_bundle(
        mut self,
        bundle: Option<SessionContextBundle>,
    ) -> Self {
        if bundle.is_some() {
            self.context_bundle = bundle;
        }
        self
    }

    /// 设置 prompt blocks。
    pub(super) fn with_prompt_blocks(mut self, blocks: Vec<serde_json::Value>) -> Self {
        self.prompt_blocks = Some(blocks);
        self
    }

    /// 设置执行器配置。
    pub(super) fn with_executor_config(mut self, config: AgentConfig) -> Self {
        self.executor_config = Some(config);
        self
    }

    /// 设置 workspace 默认值（用于 VFS 回填）。
    pub(super) fn with_workspace_defaults(mut self, workspace: Workspace) -> Self {
        self.workspace_defaults = Some(workspace);
        self
    }

    /// 可选设置 workspace 默认值。
    pub(super) fn with_optional_workspace_defaults(mut self, workspace: Option<Workspace>) -> Self {
        self.workspace_defaults = workspace;
        self
    }

    /// 设置环境变量 map（entry 注入用户侧 env）。
    pub(super) fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// 一次性吸收 `UserPromptInput` 的所有字段。
    ///
    /// 等价于依次调用 `with_prompt_blocks` / `with_executor_config` / `with_env`；
    /// 便于 entry 把"用户原始输入"集中交给 builder，compose 阶段如需要再
    /// 通过独立 `with_*` 方法覆盖个别字段（compose 产出优先）。
    pub(super) fn with_user_input(mut self, input: UserPromptInput) -> Self {
        if let Some(blocks) = input.prompt_blocks {
            self.prompt_blocks = Some(blocks);
        }
        if let Some(cfg) = input.executor_config {
            self.executor_config = Some(cfg);
        }
        self.env = input.env;
        self
    }

    /// 一步完成 companion slice 装配（VFS + MCP + 能力 + prompt + bootstrap）。
    ///
    /// 保留 `self` 上预先设置的 `env` 等字段
    /// （用 `..self` 叠加语法），只覆盖 companion slice 涉及的关注点。
    ///
    /// PR 5d（E8①）起，`parent_context_bundle` 会按 `mode` 进行 **fragment 级**
    /// 裁剪（而不是 Full 直接克隆）：`ConstraintsOnly` 只留 constraint 相关 slot，
    /// `WorkflowOnly` 只留 workflow 相关 slot，`Compact` 剔除运行时 vfs/tools
    /// 摘要类 slot 保留业务上下文，`Full` 维持完整继承。
    pub(super) fn apply_companion_slice(
        self,
        parent_vfs: Option<&Vfs>,
        parent_mcp_servers: &[agentdash_spi::SessionMcpServer],
        parent_context_bundle: Option<&SessionContextBundle>,
        mode: CompanionSliceMode,
        executor_config: AgentConfig,
        dispatch_prompt: String,
    ) -> Self {
        use crate::companion::tools::build_companion_execution_slice;

        let slice = build_companion_execution_slice(parent_vfs, parent_mcp_servers, mode);
        let flow_caps = CapabilityResolver::resolve_companion_caps(mode);

        let prompt_blocks = vec![serde_json::json!({
            "type": "text",
            "text": dispatch_prompt,
        })];

        let sliced_bundle =
            parent_context_bundle.map(|bundle| slice_companion_bundle(bundle, mode));

        Self {
            vfs: slice.vfs,
            capability_state: Some(flow_caps),
            mcp_servers: slice.mcp_servers,
            context_bundle: sliced_bundle,
            prompt_blocks: Some(prompt_blocks),
            executor_config: Some(executor_config),
            workspace_defaults: None,
            // 保留调用方已注入的 env 不被 companion slice 清空
            env: self.env,
        }
    }

    /// 一步完成 lifecycle node 装配（VFS + 能力 + MCP + prompt）。
    pub(super) fn apply_lifecycle_activation(
        mut self,
        activation: &crate::workflow::ActivityActivation,
        inherited_executor_config: Option<AgentConfig>,
    ) -> Self {
        let surface = crate::workflow::frame_builder::build_lifecycle_activation_surface(
            crate::workflow::frame_builder::AgentFrameActivationSurfaceInput {
                activation,
                base_vfs: self.vfs.as_ref(),
                inherit_skills_from: None,
            },
        );
        self.vfs = Some(surface.vfs);
        self.capability_state = Some(surface.capability_state);
        self.mcp_servers = surface.mcp_servers;
        self.prompt_blocks = Some(vec![serde_json::json!({
            "type": "text",
            "text": "请执行当前 lifecycle 节点。",
        })]);
        self.executor_config = inherited_executor_config;
        self
    }

    /// 结束 builder 链；保留该方法只为让既有 compose 代码保持声明式尾部。
    pub(super) fn build(self) -> SessionAssemblyBuilder {
        self
    }
}

/// 裁剪策略按 slot 白名单：
/// - `Full`：完整克隆父 bundle。
/// - `Compact`：剔除 `vfs` / `tools` / `persona` / `required_context` / `runtime_policy`
///   等运行时画像 slot，保留业务上下文与 workflow/约束。
/// - `WorkflowOnly`：只保留 `workflow` / `workflow_context` slot。
/// - `ConstraintsOnly`：只保留 `constraint` / `constraints` slot。
///
/// 运行期 Hook 注入不在 Bundle 中传递，子 session 由自己的 hook delegate 独立管理。
pub(super) fn slice_companion_bundle(
    parent: &SessionContextBundle,
    mode: CompanionSliceMode,
) -> SessionContextBundle {
    let keep_slot: Box<dyn Fn(&str) -> bool> = match mode {
        CompanionSliceMode::Full => Box::new(|_slot: &str| true),
        CompanionSliceMode::Compact => Box::new(|slot: &str| {
            !matches!(
                slot,
                "vfs" | "tools" | "persona" | "required_context" | "runtime_policy"
            )
        }),
        CompanionSliceMode::WorkflowOnly => {
            Box::new(|slot: &str| matches!(slot, "workflow" | "workflow_context"))
        }
        CompanionSliceMode::ConstraintsOnly => {
            Box::new(|slot: &str| matches!(slot, "constraint" | "constraints"))
        }
    };

    let mut sliced = parent.clone();
    sliced
        .bootstrap_fragments
        .retain(|fragment| keep_slot(fragment.slot.as_str()));
    sliced
}

/// 将 `SessionAssemblyBuilder` 投影到 `AgentFrameBuilder`，同时提取 launch 数据。
///
/// 替代 `apply_session_assembly`（合入 RuntimeContextInspectionPlan）的新路径。
/// frame builder 接收 surface 数据（capability/VFS/MCP），
/// 返回的 launch extras 包含 context bundle / prompt / executor config 等 launch-only 数据。
pub(super) fn project_assembly_to_frame(
    frame_builder: crate::workflow::frame_builder::AgentFrameBuilder,
    prepared: SessionAssemblyBuilder,
) -> (
    crate::workflow::frame_builder::AgentFrameBuilder,
    AssemblyLaunchExtras,
) {
    let frame_builder =
        frame_builder.with_surface_input(crate::workflow::frame_builder::AgentFrameSurfaceInput {
            capability_state: prepared.capability_state.as_ref(),
            vfs: prepared.vfs.as_ref(),
            mcp_servers: &prepared.mcp_servers,
            execution_profile: prepared.executor_config.as_ref(),
            context_bundle: prepared.context_bundle.as_ref(),
        });
    let extras = AssemblyLaunchExtras {
        context_bundle: prepared.context_bundle,
        prompt_blocks: prepared.prompt_blocks,
        executor_config: prepared.executor_config,
        mcp_servers: prepared.mcp_servers,
        vfs: prepared.vfs,
        capability_state: prepared.capability_state,
        environment_variables: prepared.env,
        workspace_defaults: prepared.workspace_defaults,
    };

    (frame_builder, extras)
}

/// `project_assembly_to_frame` 的 launch-only 输出。
///
/// 这些数据不写入 AgentFrame，而是传递给 RuntimeLaunchRequest 或 launch pipeline。
pub struct AssemblyLaunchExtras {
    pub context_bundle: Option<SessionContextBundle>,
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    pub executor_config: Option<AgentConfig>,
    pub mcp_servers: Vec<agentdash_spi::SessionMcpServer>,
    pub vfs: Option<Vfs>,
    pub capability_state: Option<CapabilityState>,
    pub environment_variables: HashMap<String, String>,
    pub workspace_defaults: Option<Workspace>,
}
