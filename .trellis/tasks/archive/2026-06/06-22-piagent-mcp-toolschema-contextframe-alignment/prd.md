# PiAgent MCP ToolSchema 与 PromptText 能力上下文收束

## Goal

收束 PiAgent 运行链路中 MCP ToolSchema 的事实源与 Agent 可见上下文路径，让 MCP 与平台内嵌 tools 使用同一套 application-owned PromptText / ContextFrame 热更新机制。

用户价值：

- 平台能全局掌握 Agent 实际收到的上下文与能力说明，而不是只依赖 provider 结构化 tools 字段。
- MCP 工具与平台内嵌工具在 Session timeline、ContextFrame、Agent 可见文本中使用同一套展示与审计路径。
- 能力热更时，模型通过追加的 PromptText 明确感知当前可用工具 schema 变化，历史对话不需要被改写。

## Confirmed Facts

- 内嵌 runtime tools 在 API bootstrap 里由 `SessionRuntimeToolComposer` 组合，当前包含 VFS、Workflow、Collaboration、Task、WorkspaceModule providers。
- VFS provider 通过 `VfsToolFactory` 按 `CapabilityState` 生成 `mounts_list`、`fs_read`、`fs_glob`、`fs_grep`、`fs_apply_patch`、`shell_exec` 等执行实例。
- session launch 调用 `assemble_tools_for_execution_context()` 构造 `context.turn.assembled_tools`，同时发现 MCP tools 并把 MCP tool 实例加入同一执行工具集合。
- PiAgent connector 会把 `assembled_tools` 写入 `agent.set_tools()`；PiAgent 的职责是持有工具表、刷新工具表并执行工具调用。
- Agent 可感知的能力热更文本来自 `ContextFrame.rendered_text`：runtime context transition 构造 `ToolSchemaDelta` 文本，launch preparation 将非 identity / system guideline / pending action 的 ContextFrame 入队为 turn-start notice。
- `HookRuntimeDelegate.transform_context()` 会消费 turn-start notice，并把这些文本作为追加 user/steering message 放入本轮上下文；测试 `transform_context_consumes_turn_start_notices_once` 已验证该路径。
- 当前 MCP discovery 本来持有 `DiscoveredMcpTool.runtime_name/server_name/tool_name/description/parameters_schema/uses_relay`，但现有 assembly 返回 `Vec<DynAgentTool>` 后丢失 MCP provenance，导致 project MCP 难以进入与内嵌 tools 一致的 ToolSchemaDelta PromptText。
- 当前前端“Agent 实际原文”展示 `ContextFrame.rendered_text`；在本任务语义下，它应继续代表 Agent 可见 PromptText，而不是 provider bridge 的完整 JSON payload。

## Requirements

- PiAgent 只拥有工具表，不负责任何能力说明文本渲染；ToolSchema PromptText 由 application 的 context projection / ContextFrame producer 生成。
- 标准能力可见路径定义为：application tool assembly 生成执行实例，同时生成 ContextFrame ToolSchema PromptText；`BridgeRequest.tools` 只作为执行/bridge 结构承载，不作为上下文说明事实源。
- MCP ToolSchema 必须进入与内嵌 tools 一致的 ContextFrame `ToolSchemaDelta.rendered_text` 路径，并通过 turn-start notice / transform_context 热更新给 Agent。
- MCP ToolSchema metadata 必须来自 MCP discovery 事实，而不是靠平台 catalog 事后猜测。
- 平台内嵌工具、平台 MCP、自定义 / project MCP 的 ToolSchema 文本使用统一格式：工具名、能力/source、tool path、description、参数摘要。
- 能力可见性变化统一按 delta 处理：初始注入等价于从空集合新增到当前可见 ToolSchema，runtime transition 等价于从上一集合变化到下一集合。
- 前端 ContextFrame 卡片能同时展示 MCP server 变化和 MCP ToolSchema PromptText，且“Agent 实际原文”能看到 MCP 工具定义文本。
- 方案只覆盖 PiAgent 主路径。

## Acceptance Criteria

- [ ] application tool assembly 边界保留 MCP discovery provenance，并能从同一批工具生成 PromptText 用 `RuntimeToolSchemaEntry`。
- [ ] PiAgent connector / agent loop 不新增 ToolSchema 文本格式化逻辑，只消费 application 提供的 ContextFrame / transform_context 输出与工具表。
- [ ] project MCP 工具进入 `ToolSchemaDelta.rendered_text`，字段至少包含 runtime tool name、description、参数摘要、`capability_key = mcp:<server>`、`source = mcp:<server>`、`tool_path = mcp:<server>::<tool>` 或等价结构化字段。
- [ ] 平台内嵌 tools 与 MCP tools 在 ContextFrame ToolSchemaDelta 中使用同一渲染函数输出 PromptText。
- [ ] 初始能力注入不使用特殊 snapshot 模型，而是按 before=empty、after=current 的普通 ToolSchema delta 生成 PromptText。
- [ ] `HookRuntimeDelegate.transform_context()` 消费后的 Agent messages 中能看到 MCP ToolSchema 文本。
- [ ] 前端 ContextFrame 展示中，“Agent 实际原文”或等价 PromptText 展开内容包含 MCP ToolSchema 文本。
- [ ] Rust focused tests 覆盖：project MCP discovery metadata 进入 ToolSchema PromptText；initial snapshot 包含 MCP ToolSchema；transition delta 热更消息包含 MCP ToolSchema。
- [ ] 前端 focused tests 覆盖：同一 ContextFrame 中 MCP server delta 与 MCP ToolSchema delta 同时展示，PromptText 展开内容包含 MCP 工具定义。
- [ ] 收尾验收必须记录架构锚定：Application owns capability facts / ToolSchema PromptText / ContextFrame rendering；PiAgent owns only tool table, tool execution, and provider bridge adaptation.

## Scope Boundary

- 本任务聚焦 PiAgent 运行链路，原因是 application 已经具备 `assembled_tools`、ContextFrame、turn-start notice 与 transform_context 的平台内控链路，PiAgent 只承接工具表和消息输入。
- 当前变更不涉及数据库结构，原因是目标是 runtime assembly、ContextFrame projection 与前端展示语义收束。
- Codex Bridge 的 app-server MCP surface 不进入本任务，原因是本次目标是先明确 PiAgent 平台标准路径。

## Architecture Anchor

- Application owns capability facts, MCP provenance, ToolSchema PromptText, ContextFrame rendering, turn-start notice enqueue, and transform_context injection.
- PiAgent owns only the executable tool table, tool refresh, tool execution, approval/update handling, and provider bridge adaptation.
- Initial capability injection is not a separate snapshot model; it is the ordinary delta from an empty visible ToolSchema set to the current visible ToolSchema set.
- MCP must enter the same application-owned ToolSchema delta and PromptText path as built-in tools.
