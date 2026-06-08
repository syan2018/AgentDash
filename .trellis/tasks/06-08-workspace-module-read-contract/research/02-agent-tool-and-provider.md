# Research: Agent 工具定义与注册 (AgentTool trait + RelayRuntimeToolProvider)

- **Query**: build_tools 如何构造工具；AgentTool trait 形态；新增 workspace_module 工具挂点与样板
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### AgentTool trait 定义

`crates/agentdash-agent-types/src/runtime/tool.rs` L48-73：

```rust
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    fn label(&self) -> &str { self.name() }   // 默认 = name
    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError>;
}
pub type DynAgentTool = Arc<dyn AgentTool>;
```

- `AgentToolResult` (L25-31)：`content: Vec<ContentPart>`, `is_error: bool`, `details: Option<serde_json::Value>`。
- `AgentToolError` (L38-46)：`ExecutionFailed(String) / InvalidArguments(String) / Other(anyhow)`。
- re-export 走 `agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback}`（canvas tools 即如此 import）。

### RelayRuntimeToolProvider::build_tools

`crates/agentdash-application/src/vfs/tools/provider.rs`。

- `RuntimeToolProvider` trait 来自 `agentdash_spi::connector`；`impl` 在 L112-332。
- struct 字段 (L30-39)：`service: Arc<VfsService>`, `repos: RepositorySet`,
  `session_services_handle: SharedSessionToolServicesHandle`, `inline_persister`,
  `function_runner`, `shell_output_registry`, `materialization`。
- `build_tools(&self, context: &ExecutionContext)` 内部模式：
  - 读 `let clusters = &context.turn.capability_state.tool.enabled_clusters;` (L136)
  - 每个工具用 `flow.is_capability_tool_enabled(CAP_*, "<tool_name>", Some(ToolCluster::X))` 门控
    （`flow = &context.turn.capability_state`, L141）。
  - Read 簇 (L143-178)：MountsListTool / FsReadTool / FsGlobTool / FsGrepTool。
  - Write 簇 (L181-194)：FsApplyPatchTool。
  - Execute 簇 (L197-216)：ShellExecTool。
  - Workflow 簇 (L219-232)：CompleteLifecycleNodeTool（`crate::workflow::tools::advance_node`）。
  - Collaboration 簇 (L235-261)：CompanionRequestTool / CompanionRespondTool。
  - Canvas 簇 (L264-328)：ListCanvasesTool / StartCanvasTool / BindCanvasDataTool /
    PresentCanvasTool —— 注意全部包在 `if let Some(project_id) = project_id_from_context(context)`。
- Capability key 常量来自 `agentdash_spi::platform::tool_capability`：
  `CAP_CANVAS, CAP_COLLABORATION, CAP_FILE_READ, CAP_FILE_WRITE, CAP_SHELL_EXECUTE, CAP_WORKFLOW`
  (import L10-12)；文件 `crates/agentdash-spi/src/platform/tool_capability.rs`。

### project_id 解析

`fn project_id_from_context(context) -> Option<Uuid>`（provider.rs L334-349）：
1. 先看 `context.turn.hook_runtime.snapshot().run_context.project_id`；
2. fallback `context.session.vfs.source_project_id` parse。
Canvas 工具就是用这个拿 project_id。

### 工具最小样板（Canvas ListCanvasesTool 范例）

`crates/agentdash-application/src/canvas/tools.rs` L23-214 是最贴近的样板（无参、按 project 列出、返回文本 + details）：

```rust
#[derive(Clone)]
pub struct ListCanvasesTool {
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
}
// fn name() -> "canvases_list" (L147)
// fn parameters_schema() -> json!({"type":"object","properties":{},"required":[],"additionalProperties":false}) (L155-162)
// execute(): repo.list_by_project(project_id) -> 排序 -> format 文本 + details json (L164-213)
```

### 新增 workspace_module_list/describe 工具的挂点结论

- **挂在同一个 `RelayRuntimeToolProvider`**：provider 已持有 `repos: RepositorySet`
  和 project_id 解析逻辑，且所有运行时工具都在这里按簇装配；新增 module 工具
  与 Canvas 工具同源（都是按 project 列资产）。无需另起 provider。
- 需要一个新 cluster 或复用现有 cluster（如 Canvas / 新 `ToolCluster::*`）来门控
  —— ToolCluster 枚举见 `agentdash_spi/src/connector/mod.rs` L193+。
- 工具实现文件可新建 `crates/agentdash-application/src/vfs/tools/` 下或新建
  `workspace_module/` 模块；构造签名按 `ListCanvasesTool::new(repo, project_id)` 模板。

## Caveats / Not Found

- 是否新增 `ToolCluster` / 新 capability key 取决于 design 选择；现状只有
  Read/Write/Execute/Workflow/Collaboration/Canvas 六簇。
