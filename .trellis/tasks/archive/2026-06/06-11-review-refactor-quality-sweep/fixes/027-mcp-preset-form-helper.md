# FIX-027: MCP Preset 表单 helper 收敛

## 模块

`mcp-preset-connectors`

## 问题

`McpPresetCategoryPanel` 和 `mcp-preset-picker` 分别维护 MCP Preset 表单默认值、客户端校验、create request 组装和 route policy 选项。两处逻辑解释同一组 generated MCP preset contract 字段，后续新增 transport 或 route policy 时容易漂移。

## 改动

- 在 `features/mcp-shared/helpers.ts` 集中 MCP Preset 表单状态、默认值、校验、create request、update patch 和 route policy options。
- `McpPresetCategoryPanel` 复用共享 helper 生成 create request 与 update patch，保留 description 清空时发送 `null` 的 tombstone 行为。
- `mcp-preset-picker` 的快速创建复用同一默认值、校验、create request 和 route policy options。
- helper 继续消费 `types/mcp-preset.ts` re-export 的 generated contract 类型，不新增重复 wire DTO。

## 涉及文件

- `packages/app-web/src/features/mcp-shared/helpers.ts`
- `packages/app-web/src/features/assets-panel/categories/McpPresetCategoryPanel.tsx`
- `packages/app-web/src/features/project/agent-preset-editor/mcp-preset-picker.tsx`

## 验证

- `rg --files packages/app-web | rg "(test|spec)\.(ts|tsx)$" | rg "mcp|preset|agent-preset|assets-panel"`：未发现现有 MCP/preset 定向测试文件。
- `pnpm --filter app-web run typecheck`：通过。

## Commit

- hash: `f797c541`
