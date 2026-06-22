# PiAgent MCP ToolSchema 与 PromptText 能力上下文收束实施计划

## Phase 1: Clarify Current Built-in Path

- [x] 在实现前确认 `SessionRuntimeToolComposer` 当前组合的 runtime providers。
- [x] 确认 `VfsToolFactory`、Workflow、Collaboration、Task、WorkspaceModule tools 都能映射为 `RuntimeToolSchemaEntry`。
- [x] 确认现有 `ToolSchemaDimensionDelta.render_text()` 输出格式作为 PromptText 标准基线。

## Phase 2: Application Assembly Surface

- [x] 在 application session tool assembly 边界引入 `AssembledToolSurface` 或等价结构，同时返回 `DynAgentTool` 与 `RuntimeToolSchemaEntry`。
- [x] runtime tool providers 生成的内嵌 tools 使用既有 platform descriptor metadata 生成 schema entries。
- [x] MCP discovery entries 直接映射为 MCP schema entries，保留 `server_name/tool_name/runtime_name/uses_relay` 的展示事实。
- [x] `context.turn.assembled_tools` 继续服务 `agent.set_tools()`；新增 schema surface 留在 application 层服务 ContextFrame PromptText。

## Phase 3: PromptText / ContextFrame Producer

- [x] 在 application ContextFrame producer 中统一实现 ToolSchema before / after delta，输出新增 / 恢复 / 移除相关 ToolSchema PromptText。
- [x] 将初始能力注入建模为 before=empty、after=current 的普通 delta，不保留单独 snapshot 生产分支。
- [x] 将 project MCP 从 platform catalog 反查路径迁出，改为 discovery metadata 直达 `RuntimeToolSchemaEntry`。
- [x] 保持 `McpServerDelta` 作为 server-level 变化 section。

## Phase 4: Transform Context Verification

- [x] 补齐 HookRuntimeDelegate focused test，验证 MCP ToolSchema ContextFrame 入队后进入 `transform_context` steering message。
- [x] 验证 transform 后的 Agent user/steering message 包含 MCP 工具名、source、tool path、参数摘要。
- [x] 复核 PiAgent connector 未新增 ToolSchema 文本渲染职责，只使用 application 注入的 messages 与 assembled tool table。

## Phase 5: Frontend Display

- [x] 确认 ContextFrame `Agent 实际原文` 展示的是 `rendered_text`，并在测试中覆盖 MCP ToolSchema 文本。
- [x] 确认 MCP ToolSchema 使用现有 `ToolSchemaDeltaBody` 渲染，chips 中可见 `mcp:<server>` source/capability。
- [x] 补齐前端 focused test：MCP server delta 与 MCP ToolSchema delta 同时存在时均可见。

## Phase 6: Validation

- [x] Rust focused test：project MCP discovery metadata 生成 `RuntimeToolSchemaEntry`。
- [x] Rust focused test：initial injection 走 before=empty、after=current delta 并包含 project MCP ToolSchema PromptText。
- [x] Rust focused test：transition delta 包含新增 / 恢复 MCP ToolSchema PromptText。
- [x] Rust focused test：`transform_context` 输出消息包含 MCP ToolSchema PromptText。
- [x] Connector focused test：PiAgent 不生成 ToolSchema PromptText，只消费 application 注入消息与工具表。
- [x] Frontend focused test：ContextFrame 卡片显示 MCP server 与 MCP ToolSchema，PromptText 展开内容包含 MCP 工具定义。
- [x] 收尾验收：把架构锚点同步到相关 Trellis spec，确认 Application/PiAgent 职责边界被长期记录。
- [x] 运行相关 Rust focused tests。
- [x] 运行 `pnpm --filter app-web test` 或收窄到 ContextFrame 相关测试。

## Validation Notes

- `cargo test -p agentdash-application session::tool_assembly --lib`：通过，1 passed。
- `cargo test -p agentdash-application session::hub::runtime_context_transition --lib`：通过，7 passed。
- `cargo test -p agentdash-application transform_context_injects_project_mcp_tool_schema_prompt_text --lib`：通过，1 passed。
- `pnpm --filter app-web test -- ContextFrameCard contextFrame`：通过，16 passed。
- 并行 implement subagent 运行 `cargo test -p agentdash-application --lib -- --skip hooks::script_engine::tests::script_reads_ctx_params`：通过，864 passed，1 filtered out。
- 并行 implement subagent 发现 `cargo test -p agentdash-application -- --nocapture` 中 `hooks::script_engine::tests::script_reads_ctx_params` 单测独立失败，断言 `None` vs `Some("params work")`，与本任务 ToolSchema/MCP/ContextFrame 改动无关。

## Closing Architecture Anchor

- Application owns capability facts, MCP provenance, RuntimeToolSchemaEntry projection, ToolSchemaDelta PromptText, ContextFrame rendering, turn-start notice enqueue, and transform_context injection.
- PiAgent owns only the executable tool table, tool table refresh, tool call execution, approval/update handling, and provider bridge adaptation.
- Initial capability injection is the ordinary delta from an empty visible ToolSchema set to the current visible ToolSchema set.
- MCP enters the same application-owned ToolSchema delta and PromptText path as built-in tools.

## Risk Points

- `DynAgentTool` 本身不携带 MCP provenance，实施时应在 assembly 阶段保留 metadata。
- `BridgeRequest.tools` 仍会存在，实施时要明确它服务 provider bridge / execution，不再被描述为平台上下文事实源。
- PiAgent connector 边界要保持窄：工具表刷新与消息投递可以在 connector 内发生，ToolSchema 文本格式化不进入 connector。
- Tool schema delta 的 key 必须稳定，避免动态 MCP server 后缀导致同一平台 MCP 每次都显示为新增。

## Staging Points

- Assembly surface 改动可先局部保留在 session preparation 内，原因是本任务只收束 PiAgent 主路径。
- Frontend 可先通过 fixture 验证 MCP ToolSchema PromptText 展示，原因是后端 contract 仍复用现有 `ToolSchemaDelta` section。
