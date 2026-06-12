# AgentRun MCP 运行时绑定

## Goal

让 Project 级 MCP Preset 可以在 AgentRun 启动时读取当前 AgentFrame final VFS 的 workspace 事实，并把这些事实注入 MCP 连接参数。核心场景是本机或 localhost HTTP MCP 需要知道当前 `main://` 工作空间对应的 P4 workspace/client，以便把同一个 MCP 服务映射到正确的物理工作范围。

## User Value

- Agent 不再只能使用静态 MCP URL/env；同一个 MCP Preset 可以随着当前 AgentRun 工作区自动绑定到正确 workspace。
- P4 workspace 名、server、stream、root 等已探测事实可被安全地投影给 MCP 服务，避免用户为每个 workspace 手工创建重复 preset。
- MCP 运行面与 AgentFrame final VFS 使用同一份事实源，减少 direct / relay / local runtime 各自猜测工作区导致的偏差。

## Confirmed Facts

- `McpPreset` 当前只保存静态 `transport` 与 `route_policy`，持久化在 `mcp_presets.transport` / `route_policy`。
- `mcp_preset_keys` 会通过 `resolve_preset_mcp_refs` 转换为 `SessionMcpServer`，`CapabilityResolver` 解析 `mcp:<preset>` directive 时也会把 preset 转换为 `SessionMcpServer`。
- `SessionMcpServer` 当前只有 `name`、`transport`、`uses_relay`，没有运行时绑定配置。
- AgentFrame final VFS 的 workspace mount 固定为 `main`，metadata 已包含 workspace id / identity payload / binding id，但没有携带 selected binding 的 `detected_facts`。
- 本机 workspace 探测已经能获取 P4 的 `client_name`、`server_address`、`user_name`、`stream`、`workspace_root`。
- relay MCP list/call 协议当前只传 `server_name`，不传 runtime-resolved transport；本机 prompt 路径会收到 `mcp_servers` 明细。
- HTTP MCP transport DTO 已有 `headers` 字段，但 direct/local 的 rmcp HTTP client 当前没有实际应用 headers。

## Requirements

1. MCP Preset 支持可选的运行时绑定配置。
   - 静态 preset 继续保持当前行为。
   - 绑定配置必须是结构化字段，不允许任意脚本或任意 JSONPath。
   - 绑定配置的目标至少支持 HTTP query、HTTP header、stdio env、stdio cwd。

2. 运行时绑定配置必须从 AgentRun runtime surface 读取事实。
   - MVP 使用当前 AgentFrame final VFS 的 `main` mount 作为工作空间事实源。
   - 可用变量包含 `vfs.main.root_ref`、`vfs.main.backend_id`、`workspace.id`、`workspace.binding_id`、`workspace.identity.*`、`workspace.detected_facts.p4.*`。
   - P4 MVP 至少覆盖 `client_name`、`server_address`、`stream`、`workspace_root`、`user_name`。
   - 缺失必需变量时 AgentRun frame construction 应失败并给出可诊断错误。

3. Preset 到 `SessionMcpServer` 的转换必须统一。
   - `mcp_preset_keys` 路径和 `mcp:<preset>` capability directive 路径必须使用同一套 resolver。
   - request/relay 透传来的 already-resolved MCP server 不应被二次解析。
   - 去重仍以 agent-facing server name 为准。

4. Direct 与 relay MCP 都必须消费同一份 resolved transport。
   - direct HTTP/SSE 发现和调用使用绑定后的 URL/header。
   - relay MCP list/call 必须把绑定后的 transport 下发到本机，不能只传 server name。
   - 本机 MCP manager 需要支持 runtime-scoped resolved server，避免不同 AgentRun runtime context 共用同名但参数不同的连接。

5. 本机和云端 HTTP MCP client 必须真正应用 headers。
   - 已有静态 header 和运行时 header 都应进入 rmcp streamable HTTP client custom headers。
   - 保留 rmcp reserved header 校验语义，不绕过协议保留头限制。

6. 前端 MCP Preset 编辑面支持配置运行时绑定。
   - Assets MCP Preset 创建/编辑表单可管理绑定条目。
   - Project Agent 快速创建 MCP Preset 的最小表单不需要完整高级配置，但不能丢失已存在的绑定配置。
   - 展示层能标记 preset 是否含运行时绑定。

7. Probe 行为保持明确。
   - 静态 preset 按当前方式 probe。
   - 含 required runtime 变量的 preset 在缺少 runtime context 的普通 preset probe 中返回 unsupported / diagnostic，而不是假装成功。
   - 后续可扩展为“带 workspace context probe”，但不作为本任务必须交付。

## Acceptance Criteria

- [ ] 数据库新增 `mcp_presets.runtime_binding`（或同等语义列）并通过 migration；repository create/read/update roundtrip 覆盖该字段。
- [ ] Rust contract DTO 与 generated TypeScript 包含 MCP runtime binding 配置，`pnpm run contracts:check` 可通过。
- [ ] `main` workspace mount metadata 携带 selected binding `detected_facts`，P4 facts 可用于 runtime binding。
- [ ] `mcp_preset_keys` 和 `mcp:<preset>` 两条路径对同一 preset 产出相同的 resolved `SessionMcpServer`。
- [ ] HTTP query/header 绑定能把 P4 client name 注入到 resolved URL/header。
- [ ] stdio env/cwd 绑定能把 workspace root 或 P4 client name 注入到本机进程环境或工作目录。
- [ ] direct MCP discovery/call 实际使用 resolved headers。
- [ ] relay MCP list/call 下发 resolved transport，本机按 runtime-scoped resolved server 建连，不因同名 server 跨 AgentRun runtime context 复用错误连接。
- [ ] 缺少 required 变量时 AgentRun frame construction 返回包含 preset key、source path、缺失变量名的错误。
- [ ] 前端 MCP Preset 编辑器能创建、编辑、展示 runtime binding 条目，并保留未编辑 preset 的既有绑定配置。
- [ ] 覆盖单元/集成测试：runtime binding resolver、capability resolver、relay protocol serialization、本机 MCP manager session-scoped key、前端表单 helpers。

## Out Of Scope

- 不支持任意脚本、Rhai、正则替换或自由 JSONPath。
- 不支持把 MCP 工具调用参数中的路径自动重写为本机路径；已有 VFS materialization 的 relay MCP arguments 契约可后续单独推进。
- 不实现 workspace-aware probe UI 的完整交互；本任务只要求普通 probe 给出明确 unsupported / diagnostic。
- 不为历史静态 preset 做自动迁移或兼容分支；新增字段为空即保持静态行为。
- 不把运行时绑定结果持久化回 preset 或 workspace。

## Assumptions

- MVP 默认只读取 `main` mount，因为当前用户场景明确指向 `main://` 工作空间；多 mount 选择后续通过 `mount_id` 字段扩展。
- 运行时绑定配置是 Project 级 preset 的一部分，而不是 Agent preset 的私有覆写；Agent 仍通过 `mcp_preset_keys` 或 `mcp:<preset>` 引用同一 project asset。
- 本任务完成规划，不启动实现；实现前需先 `task.py start`。
