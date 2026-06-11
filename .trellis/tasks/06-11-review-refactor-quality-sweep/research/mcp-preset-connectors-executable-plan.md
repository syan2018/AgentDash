# MCP Preset Connectors Executable Plan

## 模块边界

本轮只看 MCP preset / connector 配置链路。DTO 事实源整体较干净：API route 直接使用 `agentdash_contracts::mcp_preset::*`，前端 `types/mcp-preset.ts` re-export generated contracts，没有独立重复 DTO。

## 证据

- `McpPresetCategoryPanel.tsx` 的 `buildInitialForm` / `validateForm` / `buildUpdatePatch` 与 `mcp-preset-picker.tsx` 的 quick create helper 重复解释 key、display name、transport、route policy 默认值、校验和提交组装。
- `packages/app-web/src/services/mcpPreset.ts` 中 “Stdio transport 返回 unsupported” 注释已过期；后端 `mcp_preset/probe.rs` 与 API route 已支持经 relay 探测 Stdio。
- `McpPresetCategoryPanel.tsx` 的 `ToolCapsulesBody` / `ProbePanel` 与 `CapabilityPanel.tsx` 的 `mapProbeToDescriptors` 分散解释同一 `ProbeMcpPresetResponse` union。
- `CapabilityPanel.tsx` 直接调用 `probeMcpTransport`，没有复用 `stores/mcpProbeStore.ts` 的同 transport 去重缓存。
- builtin bootstrap 相关注释与实际 route 不一致，`router()` 未注册 `/bootstrap`，前端也未发现“新增内置”入口。

## 可执行批次

### Batch A: MCP preset 前端表单 helper 收敛

- 写入：`packages/app-web/src/features/mcp-shared/helpers.ts`、`McpPresetCategoryPanel.tsx`、`mcp-preset-picker.tsx`。
- 内容：抽出默认 form、校验、create request、update patch 和 route policy options。
- 风险：低；主要是表单回归。
- 验证：`pnpm --filter app-web run typecheck`。

### Batch B: probe 结果解释与缓存使用收敛

- 写入：`stores/mcpProbeStore.ts`、`McpPresetCategoryPanel.tsx`、`CapabilityPanel.tsx`、`services/mcpPreset.ts`。
- 内容：集中 probe union 到展示模型 / tool descriptor 的转换；`CapabilityPanel` 复用 probe store 去重缓存；修正 Stdio 过期注释。
- 风险：中；影响 Assets 卡片和能力面板展开工具列表。
- 验证：`pnpm --filter app-web run typecheck`，必要时 `pnpm --filter app-web test -- mcp`。

### Batch C: builtin bootstrap 死链路处理

- 写入：`crates/agentdash-api/src/routes/mcp_presets.rs`、`crates/agentdash-application/src/mcp_preset/service.rs`，若补入口再同步 contracts/service。
- 内容：若当前不支持手动新增内置，则删除或改正 `/bootstrap` 注释与残留公共意图；若支持，则补齐真实 route/request。
- 风险：中；涉及 API surface 取舍。
- 验证：`cargo test -p agentdash-application mcp_preset`；`cargo test -p agentdash-api mcp_presets`；`pnpm run contracts:check`。

## 架构项

无。当前问题可在 2-4 个局部批次内解决，不需要重定超过十个文件的跨层协议或事实源。
