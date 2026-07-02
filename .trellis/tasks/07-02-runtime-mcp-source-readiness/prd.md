# MCP 工具源 readiness 收束

## Goal

让 MCP 工具源连接失败在同一份 runtime capability surface 中被明确表达，并在本轮会话里同时对用户、模型上下文、runtime summary 可见。当前任务采用方案 B：MCP source 带 readiness 状态；接口命名和结果模型需为后续迁移到统一 `RuntimeToolSource` / 方案 C 保留空间。

## Background

之前一批提交尝试让工具无法连接时明确标识，但当前链路没有完整生效。review 发现主要问题如下：

- relay MCP discovery 在 [mcp_relay_impl.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-api/src/relay/mcp_relay_impl.rs:19) 返回 `Vec<RelayMcpToolInfo>`，失败分支只记录诊断并 `continue`，导致 [tool_assembly.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-runtime-session/src/session/tool_assembly.rs:83) 无法收到 `mcp_failures`。
- [preparation.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:97) 先克隆旧 `capability_state`，随后才在 [preparation.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:148) 写入 `unavailable_mcp_servers`，后续 [preparation.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:246)、[preparation.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:332)、[preparation.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:433) 仍使用旧拷贝。
- 本机 runtime 只在注册时通过 [ws_client.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-local/src/ws_client.rs:499) 打包 `capability_health`；虽然云端支持 [ws_handler.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-api/src/relay/ws_handler.rs:439) 接收 `EventCapabilitiesChanged`，但本机侧没有在 MCP health 变化后发送该事件。
- direct MCP discovery 在 [direct.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-executor/src/mcp/direct.rs:226) 逐 server 遍历，但 [direct.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-executor/src/mcp/direct.rs:232) 使用 `?` first-failure abort，无法表达部分成功、部分失败。
- `CapabilityState.tool.unavailable_mcp_servers: Vec<String>` 是旁路字段，不携带稳定 MCP source identity，也不满足 `CapabilityState.tool.mcp_servers` 与 `FrameLaunchEnvelope.launch_surface.mcp_servers` 同源观察的要求。

相关规范约束：

- `.trellis/spec/backend/capability/architecture.md` 要求 capability 知识不散落在 session 创建路径，运行期最终可见工具面由 AgentRun effective capability/admission 输出。
- `.trellis/spec/backend/session/session-startup-pipeline.md` 要求 `CapabilityState.tool.mcp_servers == FrameLaunchEnvelope.launch_surface.mcp_servers`，且 runtime tools 与 MCP tools 统一通过 `assemble_tool_surface_for_execution_context` 装配。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` 要求 MCP discovery provenance 与 `RuntimeToolSchemaEntry` 在 assembly 边界同源产出。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` 规定 local MCP list/call 使用 cloud 下发的 resolved `RuntimeMcpServer`，本机 static MCP catalog 不是 runtime-resolved transport 的事实源。
- `.trellis/spec/cross-layer/backbone-protocol.md` 要求平台事件保持结构化 payload，不把业务语义塞入自由文本。

## Requirements

- R1: 删除 `unavailable_mcp_servers` 旁路模型；MCP source readiness 必须与 MCP source identity 绑定，不能以自由文本列表独立存在。
- R2: `CapabilityState.tool.mcp_servers` 继续作为 MCP source surface 的唯一观察入口；每个 source 必须能表达 ready / unavailable 等 discovery 状态，并保留 server name、resolved transport、relay/direct 信息。
- R3: MCP discovery 接口必须支持 partial outcome：单个 MCP server 失败不能阻断其他 server 的工具发现，且每个 requested server 都要产出 ready 或 unavailable 状态。
- R4: relay discovery 必须把 backend anchor 缺失、backend offline、relay timeout、list_tools response error、unexpected response 等失败作为结构化 source outcome 返回，而不是只写日志。
- R5: direct discovery 必须逐 server 收集 outcome；一个 HTTP/SSE server 连接失败时，其他 direct server 仍继续发现。
- R6: `TurnPreparer` 必须只维护一份最终 `CapabilityState`；工具装配 outcome 合并后，turn supervisor、runtime transition、initial capability frame、accepted launch commit、connector context 都消费同一份最终 state。
- R7: 用户可见 MCP readiness notice 必须在 accepted commit 阶段按稳定事件顺序提交，不能在 prepare 阶段提前持久化自由文本事件。
- R8: 模型上下文必须能看到 unavailable MCP source 及其原因；context frame 渲染来自 MCP source surface，而不是从独立 failure list 拼接。
- R9: runtime summary / LocalRuntimeView 继续通过 `CapabilityHealthItem` 展示健康状态；本机 MCP health 变化后必须触发 relay capabilities changed 更新云端 registry 和 runtime health。
- R10: 设计命名应预留方案 C 迁移空间：outcome/source/status 概念不应被 MCP-only failure 命名锁死；但本任务不引入全平台 `RuntimeToolSource` 抽象。
- R11: 现有 resolved transport、protect mode、connection key、AgentRun admission、tool schema provenance 语义不得退化。
- R12: 本任务不做兼容性保留；预研项目允许直接收束到正确模型，但需要处理必要的 generated TS / contract 更新。

## Acceptance Criteria

- [ ] AC1: 代码中不再存在 `ToolDimension.unavailable_mcp_servers` / `CapabilityStateDelta.unavailable_mcp_servers` 作为独立字段。
- [ ] AC2: MCP source surface 中每个 source 带有结构化 readiness/status，且状态包含稳定 source identity 与用户/模型可读 reason。
- [ ] AC3: relay MCP list_tools 任一 server 失败时，session launch outcome 包含该 server unavailable；其他成功 server 的工具仍进入 tool surface。
- [ ] AC4: direct MCP discovery 任一 server 失败时不 first-failure abort；成功 server 的工具和失败 server 的 readiness 同时保留。
- [ ] AC5: `PreparedTurn.accepted_capability_state`、connector `ExecutionContext.turn.capability_state`、initial capability context frame 观察到同一份 MCP readiness surface。
- [ ] AC6: 用户在会话流中看到 MCP 工具源不可用提示，事件顺序位于 user input / turn_started 之后，并使用结构化 platform payload 或受控 context frame 事实。
- [ ] AC7: 模型上下文渲染明确说明 unavailable MCP source 的工具本轮不可用，以及需要修复 MCP 环境后才能使用相关工具。
- [ ] AC8: 本机 runtime MCP health 变化后向云端发送 `EventCapabilitiesChanged`，`/backends/runtime-summary` 能展示更新后的 `capability_health`。
- [ ] AC9: Rust 测试覆盖 relay partial outcome、direct partial outcome、TurnPreparer final capability state、context frame 渲染、本机 capabilities changed 推送。
- [ ] AC10: 需要生成的 TypeScript/Rust contract 文件已更新，相关 check mode 无 drift。

## Out Of Scope

- 不引入跨 builtin / workspace module / MCP / executor 的统一 `RuntimeToolSource` 抽象。
- 不重做 AgentRun admission / PermissionGrant 语义。
- 不新增数据库迁移，除非实现过程中发现现有持久化 schema 直接阻塞 runtime health 更新。
- 不保留旧 `unavailable_mcp_servers` 字段兼容路径。

## Open Questions

无阻塞问题。用户已确认采用方案 B，并要求为后续方案 C 迁移预留设计空间。
