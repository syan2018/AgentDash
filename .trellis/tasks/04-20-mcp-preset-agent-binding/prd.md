# Agent 组装面板接入 MCP Preset 活引用

## Goal

在父任务 `04-20-unified-assets-page` 完成 MCP Preset CRUD 后，把它接入 agent 组装面板：agent 配置里通过 `preset_id` **活引用** MCP Preset，运行时展开为实际 server 列表。Preset 改动 → 已引用 agent 自动生效。

## Requirements

### 后端
- `AgentConfig` / `AgentPreset.config`（`crates/agentdash-domain/src/common/agent_config.rs`）新增 `mcp_preset_refs: Vec<McpPresetRef>`（字段名以实际实现为准），与既有 inline `mcp_servers` 并存
- 运行时展开：session 启动或 agent 配置加载时，读取当前 project 的 Preset → 展开到有效 `McpServerDecl` 列表；展开失败（Preset 被删）要降级（跳过 + 日志告警，不阻塞会话启动）
- API：查询 agent 引用的 Preset、查询 Preset 被引用情况（用于 Assets 页的"引用计数"）

### 前端
- `agent-preset-editor.tsx` MCP Servers 区域新增"引用 Preset"入口：多选 Picker（从当前 project 的 MCP Preset 选），已选 Preset 以 chip 展示（点击可跳转到 Assets 查看/编辑）
- 与 inline `mcp_servers` 视觉分区：`引用的 Preset` + `内联 Server`
- Assets 页 MCP Preset 详情：显示"N 个 agent 正在引用此 Preset"

## Acceptance Criteria

- [ ] agent 配置可引用 1 个以上 MCP Preset，保存后重启会话仍生效
- [ ] 修改 Preset 的 command/env 后，新启动的 session 看到新值（不需要改 agent 配置）
- [ ] 删除被引用的 Preset：降级不阻塞，session 启动日志有 warn
- [ ] Assets 页能看到每个 Preset 的被引用 agent 数
- [ ] 后端 + 前端测试全绿

## Definition of Done

- 运行时展开逻辑有单测覆盖（正常、Preset 缺失、空列表）
- 前端 agent-preset-editor 的引用 picker 有交互测试
- 父任务产出的 Preset API 被复用，不重复实现

## Out of Scope

- Preset Bundle / 多 server 组合
- 跨项目引用
- Preset 版本锁定（首版全活引用，不提供"锁版本"选项）

## Technical Notes

**父任务产出依赖**（阻塞本任务启动）：
- `McpPreset` domain + repository + CRUD API
- Project 级作用域（同项目内引用）
- `source` 标记（builtin / user）已稳定

**待复用文件**：
- `frontend/src/features/project/agent-preset-editor.tsx:287-326` — MCP servers 编辑区
- `crates/agentdash-domain/src/common/agent_config.rs:35-56` — AgentConfig
