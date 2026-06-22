# Research: PiAgent MCP ToolSchema / ContextFrame Current Path

- Query: 复核 PiAgent 路径中 MCP ToolSchema 与标准 ToolSchema / ContextFrame / PromptText 的断层，重点确认平台内嵌 tools 进入 PromptText/ContextFrame 的路径、MCP discovery provenance 丢失点、PiAgent connector 是否承担文本渲染、initial snapshot 是否特殊建模。
- Scope: internal
- Date: 2026-06-22

## Findings

### Files Found

- `.trellis/workflow.md` — Trellis phase / research artifact workflow，要求研究结论持久化到 task research 目录。
- `.trellis/spec/backend/session/execution-context-frames.md` — connector-facing `ExecutionContext` 与 Tool Hot Update 规范，说明 PiAgent 消费 `assembled_tools`，ContextFrame 由 application 投递。
- `.trellis/spec/backend/session/session-startup-pipeline.md` — session 启动主链路与 runtime tool / MCP tool assembly 统一 helper 规范。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` — ToolCapability、MCP capability、ToolSchema PromptText 与 provider tools[] 双路径规范。
- `.trellis/spec/backend/session/pi-agent-streaming.md` — PiAgent stream / tool call 映射规范，未规定 ToolSchema 文本渲染职责。
- `.trellis/tasks/06-22-piagent-mcp-toolschema-contextframe-alignment/prd.md` — 本任务目标与验收条件。
- `.trellis/tasks/06-22-piagent-mcp-toolschema-contextframe-alignment/design.md` — 本任务目标架构草案。
- `crates/agentdash-api/src/bootstrap/session.rs` — API bootstrap 组合内嵌 runtime tool providers。
- `crates/agentdash-application/src/runtime_tools/provider.rs` — `SessionRuntimeToolComposer` 合并各 provider 的 `DynAgentTool`。
- `crates/agentdash-application/src/runtime_tools/vfs_provider.rs` — VFS provider 从 `ExecutionContext` / `CapabilityState` 构建 VFS tools。
- `crates/agentdash-application/src/vfs/tools/factory.rs` — VFS 内嵌工具按 `ToolCluster` 和 tool policy 生成。
- `crates/agentdash-application/src/session/tool_assembly.rs` — launch / refresh 共用工具装配入口，也是 MCP provenance 当前丢失点。
- `crates/agentdash-application-ports/src/mcp_discovery.rs` — MCP discovery port，当前返回带 provenance 的 `DiscoveredMcpTool`。
- `crates/agentdash-executor/src/mcp/common.rs` — MCP tool surface normalization 与 `DiscoveredMcpTool` 构造。
- `crates/agentdash-executor/src/mcp/direct.rs` — direct MCP discovery / adapter。
- `crates/agentdash-executor/src/mcp/relay.rs` — relay MCP discovery / adapter。
- `crates/agentdash-executor/src/mcp/naming.rs` — MCP runtime tool name 和 capability key 映射。
- `crates/agentdash-application/src/session/dimension/tool_schema.rs` — ToolSchemaDelta 的 `RuntimeToolSchemaEntry` 生成与 PromptText 渲染。
- `crates/agentdash-spi/src/hooks/mod.rs` — `ContextFrameSection::ToolSchemaDelta` 与 `RuntimeToolSchemaEntry` 结构。
- `crates/agentdash-application/src/session/hub/runtime_context_transition.rs` — initial / live runtime context frame 构造，ToolSchemaDelta 挂载点。
- `crates/agentdash-spi/src/connector/capability_delta.rs` — `CapabilityStateDelta` 计算，`before=None` 等价 empty baseline。
- `crates/agentdash-application/src/session/launch/preparation.rs` — turn preparation 装配 tools、构建 initial frame、把 ContextFrame 入队 transform_context。
- `crates/agentdash-application/src/session/hook_delegate.rs` — `HookRuntimeDelegate.transform_context()` 消费 turn-start notice 为 Agent-visible steering messages。
- `crates/agentdash-application/src/agent_run/frame/hook_runtime.rs` — turn-start notice 队列。
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs` — PiAgent connector 的 system prompt 拼接、工具表 set/update。
- `crates/agentdash-agent/src/agent.rs` — Agent 内部持有工具表并转为 `ToolDefinition`。
- `crates/agentdash-agent/src/agent_loop/streaming.rs` — bridge request 发送前调用 transform_context，并携带 provider `tools`。
- `crates/agentdash-agent/src/agent_loop/tool_call.rs` — agent loop 从当前工具实例刷新 `context.tools`。
- `crates/agentdash-agent-types/src/runtime/tool.rs` — `ToolDefinition::from_tool()` 只包含 name/description/parameters。
- `packages/app-web/src/features/session/model/contextFrame.ts` — 前端解析 `mcp_server_delta` / `tool_schema_delta` / `rendered_text`。
- `packages/app-web/src/features/session/ui/ContextFrameBody.tsx` — 前端显示 sections，并把 `rendered_text` 展示为“Agent 实际原文”。
- `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx` — 前端分别渲染 MCP server delta 与 ToolSchema delta。

### 1. 平台内嵌 tools 如何进入 PromptText / ContextFrame

当前平台内嵌工具的执行实例来源是 API bootstrap 注入的 `SessionRuntimeToolComposer`。bootstrap 明确组合了 VFS、Workflow、Collaboration、Task、WorkspaceModule 五类 provider：`crates/agentdash-api/src/bootstrap/session.rs:265` 到 `crates/agentdash-api/src/bootstrap/session.rs:293`。

`SessionRuntimeToolComposer` 本身只聚合执行工具实例：`build_tools()` 遍历 providers 并 `tools.extend(provider.build_tools(context).await?)`，返回 `Vec<DynAgentTool>`，见 `crates/agentdash-application/src/runtime_tools/provider.rs:79` 到 `crates/agentdash-application/src/runtime_tools/provider.rs:88`。VFS provider 再从 `ExecutionContext` 取 `CapabilityState` / VFS，调用 `VfsToolFactory::build_tools()`，见 `crates/agentdash-application/src/runtime_tools/vfs_provider.rs:60` 到 `crates/agentdash-application/src/runtime_tools/vfs_provider.rs:84`。VFS factory 根据 `ToolCluster` 与 `is_capability_tool_enabled()` 生成 `mounts_list`、`fs_read`、`fs_glob`、`fs_grep` 等实例，见 `crates/agentdash-application/src/vfs/tools/factory.rs:46` 到 `crates/agentdash-application/src/vfs/tools/factory.rs:95`。

launch preparation 在 connector 启动前把工具装配到 connector context：`context.turn.assembled_tools = deps.assemble_tools(&session_id, &context).await`，见 `crates/agentdash-application/src/session/launch/preparation.rs:98` 到 `crates/agentdash-application/src/session/launch/preparation.rs:100`。owner bootstrap 时 initial capability frame 使用同一批 `context.turn.assembled_tools` 构建，见 `crates/agentdash-application/src/session/launch/preparation.rs:237` 到 `crates/agentdash-application/src/session/launch/preparation.rs:244`。

ToolSchema PromptText 当前不是 provider 直接产出，而是 application 在 `ToolSchemaDimensionDelta` 中从 `DynAgentTool` 反抽 schema：`runtime_tool_schema_entries_from_tools()` 先 `ToolDefinition::from_tool()`，排序去重后转为 `RuntimeToolSchemaEntry`，见 `crates/agentdash-application/src/session/dimension/tool_schema.rs:94` 到 `crates/agentdash-application/src/session/dimension/tool_schema.rs:107`。`RuntimeToolSchemaEntry` 包含 `name`、`description`、`parameters_schema`、`capability_key`、`source`、`tool_path`、`context_usage_kind`，见 `crates/agentdash-spi/src/hooks/mod.rs:431` 到 `crates/agentdash-spi/src/hooks/mod.rs:445`。

内嵌工具的 `capability_key/source/tool_path/context_usage_kind` 由静态 `platform_tool_descriptors()` 补充。`ToolSource` 区分 `Platform`、`PlatformMcp`、`Mcp`，见 `crates/agentdash-spi/src/platform/tool_capability.rs:155` 到 `crates/agentdash-spi/src/platform/tool_capability.rs:179`；静态 descriptor 列表从 `platform_tool_descriptors()` 返回，见 `crates/agentdash-spi/src/platform/tool_capability.rs:257` 到 `crates/agentdash-spi/src/platform/tool_capability.rs:270`，平台 MCP descriptor 也在同一静态 catalog 中声明，例见 `crates/agentdash-spi/src/platform/tool_capability.rs:384` 到 `crates/agentdash-spi/src/platform/tool_capability.rs:392`。

`ToolSchemaDimensionDelta.render_text()` 是当前 Agent-visible PromptText 的渲染点：它输出 `## Tool Schema Delta`，并对每个新增工具调用 `format_tool_schema_entry()` 输出工具名、capability/source/path、description 和参数摘要，见 `crates/agentdash-application/src/session/dimension/tool_schema.rs:74` 到 `crates/agentdash-application/src/session/dimension/tool_schema.rs:88` 以及 `crates/agentdash-application/src/session/dimension/tool_schema.rs:109` 到 `crates/agentdash-application/src/session/dimension/tool_schema.rs:131`。

该文本随后进入 `ContextFrame.rendered_text`：`RuntimeContextUpdateFrame.rendered_text()` 将所有 dimension 的 `render_text()` join 到 frame，见 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:554` 到 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:561`。launch preparation 会把非 identity / system_guidelines / pending_action 的 frame 入队为 turn-start notice，见 `crates/agentdash-application/src/session/launch/preparation.rs:413` 到 `crates/agentdash-application/src/session/launch/preparation.rs:439`。

最终 `HookRuntimeDelegate.transform_context()` 消费 turn-start notice，把 ContextFrame 文本作为追加 user steering message 放进本轮消息：收集 notice 见 `crates/agentdash-application/src/session/hook_delegate.rs:871` 到 `crates/agentdash-application/src/session/hook_delegate.rs:904`，格式化 `[CTX Frame]` 见 `crates/agentdash-application/src/session/hook_delegate.rs:906` 到 `crates/agentdash-application/src/session/hook_delegate.rs:915`，追加 messages 见 `crates/agentdash-application/src/session/hook_delegate.rs:505` 到 `crates/agentdash-application/src/session/hook_delegate.rs:523`。

前端展示也以 ContextFrame 为标准路径：`ContextFrameBody` 先渲染 `frame.sections`，再把 `frame.rendered_text` 展示为“Agent 实际原文”，见 `packages/app-web/src/features/session/ui/ContextFrameBody.tsx:21` 到 `packages/app-web/src/features/session/ui/ContextFrameBody.tsx:28` 以及 `packages/app-web/src/features/session/ui/ContextFrameBody.tsx:33` 到 `packages/app-web/src/features/session/ui/ContextFrameBody.tsx:48`。

### 2. MCP discovery entries 在哪里丢失 provenance

MCP discovery port 当前并不缺 provenance。`DiscoveredMcpTool` 已携带 `runtime_name`、`server_name`、`tool_name`、`uses_relay`、`description`、`parameters_schema` 和 `tool`，见 `crates/agentdash-application-ports/src/mcp_discovery.rs:6` 到 `crates/agentdash-application-ports/src/mcp_discovery.rs:15`。

direct / relay 两条 MCP 路径都先构建 `McpToolSurface`，其中 `runtime_name`、`server_name`、`tool_name`、description、sanitized schema 都被保留，见 `crates/agentdash-executor/src/mcp/common.rs:8` 到 `crates/agentdash-executor/src/mcp/common.rs:32`。再通过 `build_discovered_entry()` 写入 `DiscoveredMcpTool`，见 `crates/agentdash-executor/src/mcp/common.rs:57` 到 `crates/agentdash-executor/src/mcp/common.rs:70`。

direct MCP adapter 从 rmcp tool 读 `tool.name`、`tool.description`、`tool.input_schema`，并把 adapter 暴露为 `AgentTool` 的 `runtime_name/description/parameters_schema`，见 `crates/agentdash-executor/src/mcp/direct.rs:145` 到 `crates/agentdash-executor/src/mcp/direct.rs:174`。direct discovery 还基于 `capability_key_for_mcp_server_name()` 与 `CapabilityState.is_capability_tool_enabled()` 做过滤，然后 push `build_discovered_entry(...)`，见 `crates/agentdash-executor/src/mcp/direct.rs:221` 到 `crates/agentdash-executor/src/mcp/direct.rs:249`。

relay MCP adapter 同样从 relay tool info 保留 `server_name/tool_name/description/parameters_schema`，见 `crates/agentdash-executor/src/mcp/relay.rs:31` 到 `crates/agentdash-executor/src/mcp/relay.rs:64`；relay discovery 对 requested server 与 capability tool policy 过滤后返回 `build_discovered_entry(...)`，见 `crates/agentdash-executor/src/mcp/relay.rs:117` 到 `crates/agentdash-executor/src/mcp/relay.rs:146`。

丢失点在 application assembly 边界：`assemble_tools_for_execution_context()` 调用 `discover_tool_entries(...)` 后只执行 `all_tools.extend(entries.into_iter().map(|entry| entry.tool))`，见 `crates/agentdash-application/src/session/tool_assembly.rs:24` 到 `crates/agentdash-application/src/session/tool_assembly.rs:45`。返回类型是 `Vec<DynAgentTool>`，见 `crates/agentdash-application/src/session/tool_assembly.rs:6` 到 `crates/agentdash-application/src/session/tool_assembly.rs:11` 和返回 `all_tools` 的 `crates/agentdash-application/src/session/tool_assembly.rs:48`。

丢失后，ToolSchemaDelta 只能从 `ToolDefinition` 反推 schema。`ToolDefinition` 只有 `name`、`description`、`parameters`，见 `crates/agentdash-agent-types/src/runtime/tool.rs:14` 到 `crates/agentdash-agent-types/src/runtime/tool.rs:20`；`ToolDefinition::from_tool()` 也只提取 `tool.name()`、`tool.description()`、`tool.parameters_schema()`，见 `crates/agentdash-agent-types/src/runtime/tool.rs:77` 到 `crates/agentdash-agent-types/src/runtime/tool.rs:85`。因此 `server_name/tool_name/uses_relay` 这类 MCP provenance 在转为 `DynAgentTool` 后不再可见。

`ToolSchemaDimensionDelta` 当前补 metadata 的机制只查 `platform_tool_descriptors()`，见 `crates/agentdash-application/src/session/dimension/tool_schema.rs:335` 到 `crates/agentdash-application/src/session/dimension/tool_schema.rs:379`。runtime name 映射对 `PlatformMcp` 使用稳定 server namespace，见 `crates/agentdash-application/src/session/dimension/tool_schema.rs:399` 到 `crates/agentdash-application/src/session/dimension/tool_schema.rs:424`。这能覆盖平台内嵌工具和平台 MCP，但 project/custom MCP 的工具列表来自运行期 discovery，不在静态 descriptor catalog 中；因此 project MCP schema 即使已进入 `agent.set_tools()`，也无法可靠进入带 `capability_key/source/tool_path` 的 ToolSchema PromptText。

### 3. PiAgent connector 是否存在 ToolSchema / PromptText 文本渲染职责

当前 PiAgent connector 持有可执行工具表并负责 set/update。新建或重建 agent 时，它从 `context.turn.assembled_tools` 设置 `current_tools`，再 `agent.set_tools(current_tools.clone())`，见 `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:737` 到 `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:755`。热更新时 `update_session_tools()` replace-set `runtime.tools` 并调用 `runtime.agent.set_tools(tools)`，见 `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:966` 到 `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:989`。

PiAgent connector 当前唯一明显的文本拼接职责是 system prompt：`incoming_system_prompt = assemble_system_prompt(&context.turn.context_frames)`，见 `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:660`；`assemble_system_prompt()` 只按 `identity` 与 `system_guidelines` 两类 frame 的 `rendered_text` 拼接，见 `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:1045` 到 `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:1064`。这不是 ToolSchema PromptText 渲染。

agent runtime 内部也只是把工具实例转换为 provider bridge 的结构化 tools。`Agent::set_tools()` 只替换 live/state 工具表，见 `crates/agentdash-agent/src/agent.rs:144` 到 `crates/agentdash-agent/src/agent.rs:151`。prompt 时读取 live tools 并转为 `ToolDefinition::from_tool()`，构造 `AgentContext.tools`，见 `crates/agentdash-agent/src/agent.rs:453` 到 `crates/agentdash-agent/src/agent.rs:473`。agent loop 在发送前刷新 `context.tools`，见 `crates/agentdash-agent/src/agent_loop/tool_call.rs:436` 到 `crates/agentdash-agent/src/agent_loop/tool_call.rs:451`。

发送给 provider 的 `BridgeRequest` 同时带 `system_prompt`、transform 后的 `messages` 和 `tools: context.tools.clone()`，见 `crates/agentdash-agent/src/agent_loop/streaming.rs:117` 到 `crates/agentdash-agent/src/agent_loop/streaming.rs:146`。这里的 `tools` 是 provider / bridge 机器 schema 承载，不是 application-owned ContextFrame PromptText；PromptText 注入发生在同一段代码之前的 `runtime_delegate.transform_context(...)`，见 `crates/agentdash-agent/src/agent_loop/streaming.rs:117` 到 `crates/agentdash-agent/src/agent_loop/streaming.rs:134`。

结论：PiAgent connector 当前没有 project MCP ToolSchema PromptText 的专门渲染职责；问题不应通过在 connector 内新增 ToolSchema 文本格式化来修复。正确断点在 application tool assembly / ContextFrame producer 之间，让 connector 继续只消费工具表和 transform 后的 messages。

### 4. Initial snapshot 是否特殊建模

当前 initial capability frame 一半是普通 delta，一半仍有特殊 snapshot 语义。普通 delta 的部分：`build_initial_capability_state_frame()` 用 `compute_capability_state_delta(None, capability_state, capability_keys)` 计算初始差异，见 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:74` 到 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:94`。`compute_capability_state_delta()` 在 `before=None` 时把 before capabilities / clusters / paths / MCP servers / companion / VFS / skills 都视为空，见 `crates/agentdash-spi/src/connector/capability_delta.rs:100` 到 `crates/agentdash-spi/src/connector/capability_delta.rs:161`。这已经符合 “before=empty -> after=current” 的差量计算方式。

特殊建模的部分：`RuntimeContextUpdateFrame.kind()` 当 `apply_mode == Some("initial")` 时返回 `"capability_state_snapshot"`，否则返回 `"capability_state_delta"`，见 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:521` 到 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:532`。测试也断言 initial 返回 snapshot kind，见 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:730` 到 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:751`。

另一个 initial 特殊分支在 companion roster：`should_include_companion_state_section()` 通过 `initial_snapshot = matches!(apply_mode, Some("initial"))` 决定在 initial 时输出 roster 状态，见 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:470` 到 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:490`。这不是 ToolSchema 专属问题，但说明 initial 在 runtime context frame 层仍被特殊看待。

结论：若目标是 “Initial snapshot 不特殊化，本质是 before=empty -> after=current visible ToolSchema set 的普通 delta”，当前需要移除或收束 `capability_state_snapshot` kind / `apply_mode == initial` 的特殊语义，至少 ToolSchema 维度应走与 live delta 完全一致的 before/after schema set 逻辑。

### Current Gap Summary

当前标准路径已经存在：

```text
RuntimeToolProvider + MCP discovery
  -> assemble_tools_for_execution_context
  -> context.turn.assembled_tools
  -> PiAgent agent.set_tools / update_session_tools
```

以及：

```text
RuntimeContextUpdateFrame
  -> ToolSchemaDimensionDelta.render_text()
  -> ContextFrame.rendered_text
  -> turn-start notice
  -> HookRuntimeDelegate.transform_context()
  -> Agent-visible steering message
```

断层在两条路径之间的事实形状：`assemble_tools_for_execution_context()` 只输出 `Vec<DynAgentTool>`，MCP discovery 的 provenance 没有随工具 surface 留在 application 层；ToolSchemaDelta 只能从 `DynAgentTool -> ToolDefinition` 重建 schema，并靠静态 platform catalog 补 metadata。平台内嵌 tools 和平台 MCP 因为有静态 descriptor，可以进入较完整的 ToolSchemaDelta；project/custom MCP 只能保留 runtime tool name / description / parameters，无法可靠保留 `capability_key = mcp:<server>`、`source = mcp:<server>`、`tool_path = mcp:<server>::<tool>`。

运行中热更新同样经过这个断点：`hub/tool_builder.rs` 采用新 frame 后重新 assemble tools，先 `connector.update_session_tools(session_id, all_tools.clone())`，再把 `&all_tools` 传给 `emit_adopted_runtime_context_transition(...)`，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:252` 到 `crates/agentdash-application/src/session/hub/tool_builder.rs:289`。如果只在 PiAgent connector 补文本，ContextFrame、前端和 application 审计仍然缺 MCP ToolSchema PromptText。

## External References

- 未使用外部文档。本轮只读研究基于仓库代码、Trellis specs 和任务文档。

## Related Specs

- `.trellis/spec/backend/session/execution-context-frames.md` — `ExecutionContext`、connector consumption matrix、Tool Hot Update、PiAgent bundle handling。
- `.trellis/spec/backend/session/session-startup-pipeline.md` — `LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan -> PreparedTurn`，以及 `assemble_tools_for_execution_context` 是 prompt 前工具面与运行中 refresh 的统一 helper。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` — `ToolCapability` / `mcp:<preset>` / `ToolSchema` 与 PromptText 规范。
- `.trellis/spec/backend/session/pi-agent-streaming.md` — PiAgent stream event / tool call 映射，不扩展 ToolSchema 文本职责。
- `.trellis/spec/frontend/index.md` 与 `.trellis/spec/frontend/quality-guidelines.md` — 前端 context frame 展示属于 session UI 质量约束范围；本轮未深入读取全部 frontend spec，因为研究重点是后端断层。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本研究使用用户明确给出的任务目录 `.trellis/tasks/06-22-piagent-mcp-toolschema-contextframe-alignment` 写入，没有从环境猜测 active task。
- 本轮未运行测试，也未做代码修改；只写入本 research 文件。
- 未研究 Codex Bridge 路径；用户明确要求不考虑 Codex Bridge。
- 未发现 PiAgent connector 内存在 project MCP ToolSchema PromptText 专门渲染逻辑；只发现 identity / system_guidelines system prompt 拼接和 provider bridge `tools` 机器 schema 承载。
- 未发现当前 assembly surface 中有 `RuntimeToolSchemaEntry` 与 `DynAgentTool` 同源返回结构；当前相关函数返回值仍是 `Vec<DynAgentTool>`。
