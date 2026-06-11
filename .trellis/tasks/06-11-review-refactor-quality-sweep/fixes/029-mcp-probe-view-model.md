# Fix 029: MCP probe view model 收敛

## 问题

- Assets MCP Preset 卡片/详情与 Workflow CapabilityPanel 分散解释 `ProbeMcpPresetResponse` union，工具展示文案和 `ToolDescriptor` 映射存在重复。
- Workflow CapabilityPanel 展开 `mcp:<preset>` 工具列表时直接调用 probe service，没有复用 `mcpProbeStore` 已有的同 transport 缓存与 inflight 去重。
- `services/mcpPreset.ts` 对 stdio probe 的注释仍描述为 unsupported，已不符合当前经本机 relay 探测的实现。

## 改动

- 新增 `features/mcp-shared/probeViewModel.ts`，集中生成 MCP probe 展示模型、工具描述符和占位工具描述符。
- Assets MCP Preset 卡片与详情面板改为消费 probe view model，保留原有“尚未探测 / 探测中 / 成功 / 失败 / unsupported”展示行为。
- `mcpProbeStore` 新增 `getOrRefresh`，缓存命中直接返回，未命中时复用原 `refresh` 的 inflight 去重。
- Workflow CapabilityPanel 展开 MCP 工具时改用 `getOrRefresh`，并复用共享 `ToolDescriptor` 映射 helper。
- 修正 `probeMcpTransport` 注释，说明 stdio 通过本机 relay 探测，http/sse 使用 URL 直连探测。

## 涉及文件

- `packages/app-web/src/features/mcp-shared/probeViewModel.ts`
- `packages/app-web/src/stores/mcpProbeStore.ts`
- `packages/app-web/src/features/assets-panel/categories/McpPresetCategoryPanel.tsx`
- `packages/app-web/src/features/workflow/ui/panels/CapabilityPanel.tsx`
- `packages/app-web/src/services/mcpPreset.ts`

## 验证

- `pnpm --filter app-web run typecheck`：通过。
- `git diff --check`：通过。
- MCP 相关定向测试：未运行，`rg --files packages/app-web | rg '(mcp|Mcp).*(test|spec)|(__tests__).*mcp|mcp.*(__tests__)'` 未发现匹配测试文件。

## Commit

- hash: `cdd47f46`
