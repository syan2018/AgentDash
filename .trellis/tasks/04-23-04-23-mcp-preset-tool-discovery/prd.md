# MCP Preset 工具发现与连通性检查

## Goal

为 MCP Preset 补齐**运行时工具发现**（`tools/list`）和**连通性检查**（test connection）能力。
当前 MCP Preset 只有静态 CRUD，前端无法查看某个 Preset 对应的 MCP Server 实际提供了哪些工具，
也无法在配置阶段验证 Server 是否可达——错误只在 agent session 启动时才暴露，用户体验差。

## What I already know

### 已有实现

- **域模型完整**：`McpPreset` entity + `McpTransportConfig`（Http/Sse/Stdio）+ `McpRoutePolicy`（Auto/Relay/Direct）
- **CRUD API 完整**：`/api/projects/{project_id}/mcp-presets` 全套 REST + bootstrap + clone
- **前端 CRUD 完整**：`McpPresetCategoryPanel.tsx`（卡片网格 + detail dialog + transport editor）
- **执行器内部有 tool discovery**：`pi_agent_mcp.rs::discover_mcp_tools()` 在 session 启动时连接 HTTP MCP Server 并 `list_all_tools()`，但仅限内部使用，不暴露给 API/前端
- **Local 端有 client manager**：`mcp_client_manager.rs::McpClientManager` 管理 stdio/http 长连接，支持 `list_tools()` / `call_tool()`，但仅服务于 relay 通道
- **tool_catalog 返回占位符**：对 `mcp:*` capability key，返回 "需运行时发现具体工具列表" 的 placeholder

### 缺失能力

1. **无 API 层工具发现端点**——前端无法触发 `tools/list` 获取实际工具列表
2. **无连通性检查端点**——配置完成后无法验证 Server 是否可连通
3. **前端无工具列表展示**——MCP Preset detail 面板不展示实际可用工具
4. **前端无连通性状态**——无 "Test Connection" 按钮或状态指示

### 架构约束

- **Stdio transport 只能在 local 端执行**（需要 spawn 子进程），云端 API 无法直接连接 stdio MCP Server
- **Http/Sse transport 可在云端直连**（executor 已有 `connect_http_server` 模式）
- **Route Policy 决定运行时路由**：`auto` 模式下 stdio → relay，http/sse → direct；`relay` 强制 relay；`direct` 强制直连
- `rmcp` crate 已作为依赖存在（client + server 功能均已启用）

## Requirements

### R1: 后端 — Probe 端点（工具发现 + 连通性检查合一）

新增 API 端点：

```
POST /api/projects/{project_id}/mcp-presets/{id}/probe
```

**行为**：
- 根据 preset 的 transport config 建立临时 MCP 连接
- 调用 MCP `tools/list` 获取工具列表
- 连接成功 + 工具列表获取成功 = 连通性 OK
- 连接失败或超时 = 连通性 FAIL，返回错误信息

**返回结构**：
```json
{
  "status": "ok" | "error",
  "latency_ms": 1234,
  "tools": [
    { "name": "read_file", "description": "Read file content" }
  ],
  "error": null | "连接超时: ..."
}
```

**Transport 处理**：
- **Http / Sse**：云端直连（复用 executor 的 `rmcp` StreamableHttp client 模式）
- **Stdio**：当前阶段返回 `{ "status": "unsupported", "error": "Stdio transport 需要通过本地 relay 探测，当前暂不支持" }`。后续可通过 relay WebSocket 通道下发 probe 指令给 local 端。

**约束**：
- 超时上限 15 秒
- 连接使用后立即关闭（临时连接，不复用）
- 需要 project View 权限

### R2: 前端 — MCP Preset 详情面板增加 Probe 功能

在 `McpPresetDetailDialog` 中：
- 新增 **"Test Connection"** 按钮（对话框底部或 transport 区域旁）
- 点击后调用 probe API，展示加载态
- 成功：展示绿色状态 + 延迟 + 工具列表（name + description 表格）
- 失败：展示红色状态 + 错误信息
- Stdio：按钮 disabled 并 tooltip 提示 "Stdio 暂不支持云端探测"

### R3: 前端 — Workflow 编辑器 CapabilitiesEditor MCP 工具列表集成

当前 `CapabilitiesEditor` 展开 `mcp:*` capability 时调用 `fetchToolCatalog(["mcp:xxx"])`，
后端 `query_tool_catalog` 对 MCP key 只返回一个占位符 ToolDescriptor。

**改进**：展开 `mcp:*` capability 时，改为调用 probe 端点获取真实工具列表，
将 probe 返回的 `{ name, description }` 映射为 `ToolDescriptor[]` 格式（`source: { type: "mcp", server_name }`)，
供 `ToolListPanel` / `ToolRow` 消费。

**具体行为**：
- 展开 mcp capability → 调用 `probeMcpPreset(projectId, presetId)` （需先从 preset key 查 preset id）
- probe 成功 → 缓存并展示真实工具列表（每个工具可 block/unblock）
- probe 失败 → 展示错误提示（如 "MCP Server 不可达"），保留占位符
- Stdio → 仍展示占位符 + 提示 "Stdio 暂不支持云端探测"
- 工具列表缓存策略：session 内缓存（已有 `toolCatalogCache` state），不跨页持久化

**涉及组件**：
- `workflow-editor.tsx` 的 `CapabilitiesEditor.toggleExpand()` （line 816-837）
- 已有 `mcpPresets` state（line 773-795，从 `fetchProjectMcpPresets` 加载）可用于 key → id 映射

### R4: 前端 — MCP Preset 卡片展示连通状态（可选延伸）

在卡片网格中，对已 probe 过的 preset 显示小图标指示上次探测状态。
此项为 nice-to-have，MVP 可不做。

## Acceptance Criteria

- [ ] `POST .../mcp-presets/{id}/probe` 端点对 Http/Sse transport preset 返回正确工具列表
- [ ] probe 端点对 Stdio transport 返回 unsupported 状态
- [ ] probe 端点在目标 server 不可达时返回 error 状态和有意义的错误信息
- [ ] probe 端点遵守 15 秒超时
- [ ] 前端 MCP Preset detail dialog 有 "Test Connection" 按钮并正确展示三种状态（ok/error/unsupported）
- [ ] 前端 ok 状态下展示工具列表（name + description）
- [ ] Stdio transport 的 preset 在 UI 中明确提示不支持云端探测
- [ ] Workflow 编辑器 CapabilitiesEditor 展开 mcp:* capability 时调用 probe 展示真实工具列表
- [ ] Workflow 编辑器中 probe 失败时展示错误提示而非空白

## Definition of Done

- 后端新增路由 + handler + 单元测试
- 前端组件更新 + 服务函数新增
- Lint / typecheck / CI green
- 手动验证至少一个 Http MCP Server 的 probe 成功路径

## Out of Scope

- Stdio transport 的 relay 端 probe（需要扩展 relay 协议，后续单独规划）
- 定期自动 probe / 健康检查巡检
- 工具列表持久化缓存（每次点击都是实时探测）
- MCP Preset 卡片级连通状态图标（R3 为 nice-to-have，不在 MVP 内）
- 后端 `query_tool_catalog` 改造（仍返回 placeholder，真实工具由前端调 probe 获取）
- MCP Preset 卡片级连通状态图标（R4 为 nice-to-have，不在 MVP 内）

## Technical Approach

### 后端

1. **新建 probe 模块**：`crates/agentdash-application/src/mcp_preset/probe.rs`
   - `probe_mcp_preset(preset: &McpPreset) -> ProbeResult`
   - 对 Http/Sse：使用 `rmcp` 的 `StreamableHttpClientWorker` 建立临时连接 → `list_all_tools()` → 关闭
   - 对 Stdio：返回 `ProbeResult::Unsupported`
   - 包裹 `tokio::time::timeout(Duration::from_secs(15), ...)` 超时保护

2. **新增 API 路由**：
   - handler：`crates/agentdash-api/src/routes/mcp_presets.rs` 新增 `probe_mcp_preset` handler
   - 路由注册：`routes.rs` 追加 `/projects/{project_id}/mcp-presets/{id}/probe`
   - DTO：`crates/agentdash-api/src/dto/mcp_preset.rs` 新增 `ProbeResponse`

### 前端

3. **新增服务函数**：`frontend/src/services/mcpPreset.ts` 新增 `probeMcpPreset(projectId, presetId)`

4. **更新 MCP Preset detail dialog**：`McpPresetCategoryPanel.tsx` 的 `McpPresetDetailDialog` 中新增 probe 按钮和结果展示区

5. **更新 Workflow 编辑器 CapabilitiesEditor**：`workflow-editor.tsx` 的 `toggleExpand()` 逻辑
   - 当展开的 key 以 `mcp:` 开头时，从已加载的 `mcpPresets` state 中按 key 查到 preset id
   - 调用 `probeMcpPreset(projectId, presetId)` 替代 `fetchToolCatalog([key])`
   - 将 probe 结果中的 `tools[]` 映射为 `ToolDescriptor[]`：
     ```ts
     probeResult.tools.map(t => ({
       name: t.name,
       display_name: t.name,
       description: t.description,
       source: { type: "mcp" as const, server_name: mcpPresetKey },
       capability_key: key,
     }))
     ```
   - probe 失败时设置 `toolCatalogCache[key]` 为含单个错误占位符的数组
   - Stdio unsupported 同理

### 关键参考文件

| 层 | 文件 | 作用 |
|---|---|---|
| domain | `crates/agentdash-domain/src/mcp_preset/value_objects.rs` | transport 类型定义 |
| app | `crates/agentdash-application/src/mcp_preset/service.rs` | 现有 service |
| app (新) | `crates/agentdash-application/src/mcp_preset/probe.rs` | probe 逻辑 |
| executor (参考) | `crates/agentdash-executor/src/connectors/pi_agent/pi_agent_mcp.rs` | 现有 HTTP MCP 连接模式 |
| local (参考) | `crates/agentdash-local/src/mcp_client_manager.rs` | 现有 client 管理模式 |
| api | `crates/agentdash-api/src/routes/mcp_presets.rs` | 现有路由 handler |
| api | `crates/agentdash-api/src/routes.rs` | 路由注册 |
| api | `crates/agentdash-api/src/dto/mcp_preset.rs` | DTO 定义 |
| frontend | `frontend/src/services/mcpPreset.ts` | API client |
| frontend | `frontend/src/features/assets-panel/categories/McpPresetCategoryPanel.tsx` | UI 面板 |
| frontend types | `frontend/src/types/mcp-preset.ts` | 前端类型 |
| frontend types | `frontend/src/types/workflow.ts` | ToolDescriptor 定义（line 243-252） |
| frontend | `frontend/src/features/workflow/workflow-editor.tsx` | CapabilitiesEditor（line 751）+ toggleExpand（line 816） |
| frontend | `frontend/src/services/workflow.ts` | fetchToolCatalog（line 642） |
| backend | `crates/agentdash-api/src/routes/workflows.rs` | query_tool_catalog handler（line 739） |
| backend | `crates/agentdash-application/src/capability/tool_catalog.rs` | MCP placeholder 逻辑 |

## Technical Notes

- `rmcp` crate 已有 `StreamableHttpClientWorker` 和 `list_all_tools()` API，probe 实现可直接复用
- executor 中 `connect_http_server()` + `client.list_all_tools()` + `client.cancel()` 的模式即为 probe 的核心逻辑
- SSE transport 在 `rmcp` 中也走 `StreamableHttpClientWorker`（url-based），与 Http 处理一致
- probe 是临时连接模式（connect → list → cancel），不需要管理连接池
- 前端现有 `McpPresetDetailDialog` 已有 transport 展示区域，probe 按钮可放在 transport section 旁
