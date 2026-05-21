# 实施：前端设计语言批量迁移

## 执行顺序

```
Phase A — primitive 上提与基线（主会话 inline，顺序）
    A1 → A2 → A3 → A4 → A5 → 验证 (eslint+typecheck) → 暂不 commit

Phase B — 业务文件并行清扫（7 个 trellis-implement subagent 并发）
    SA-1..SA-7 同时启动 → 全部回收 → 主会话验证 (eslint+typecheck+test)

Phase C — 收口（主会话 inline）
    C1 spec 同步 → C2 等用户视觉验收 → C3（用户批准后）commit + finish-work
```

## Phase A — 基础设施迁移

### A1 — 上提 DetailPanel 系列到 @agentdash/ui

- [ ] 新建 [packages/ui/src/primitives/DetailPanel.tsx](packages/ui/src/primitives/DetailPanel.tsx)，从 `app-web/src/components/ui/detail-panel.tsx` 迁移：DetailPanel / DetailSection / DetailMenu / DangerConfirmDialog。
- [ ] 修复内部违规：
  - `rounded-[10px]` → `rounded-[8px]`（DetailMenu 按钮、DetailPanel 关闭按钮）
  - `rounded-[16px]` → `rounded-[12px]`（DangerConfirmDialog 主容器）
  - 其它 `rounded-[12px]` 保持
- [ ] 在 [packages/ui/src/index.ts](packages/ui/src/index.ts) 导出。
- [ ] 删除 [packages/app-web/src/components/ui/detail-panel.tsx](packages/app-web/src/components/ui/detail-panel.tsx)。
- [ ] 调用方批量改 import：`@/components/ui/detail-panel` → `@agentdash/ui`（12 个文件）。

### A2 — 上提 CardMenu 到 @agentdash/ui

- [ ] 新建 [packages/ui/src/primitives/CardMenu.tsx](packages/ui/src/primitives/CardMenu.tsx)。
- [ ] 修复内部违规：`bg-amber-500/15 text-amber-600 dark:text-amber-400` → `bg-warning/15 text-warning`；`rounded-[10px]` → `rounded-[8px]`。
- [ ] 在 `@agentdash/ui` 导出。
- [ ] 删除 [packages/app-web/src/features/assets-panel/_shared/CardMenu.tsx](packages/app-web/src/features/assets-panel/_shared/CardMenu.tsx)。
- [ ] 4 个调用方改 import。

### A3 — 移除 SourceBadge，调用方迁到 OriginBadge

- [ ] 在 2 个调用方（McpPresetCategoryPanel、WorkflowCategoryPanel）替换：
  - `<SourceBadge variant="marketplace" />` → `<OriginBadge tone="success" label="marketplace" />`
  - `<SourceBadge variant="builtin" />` → `<OriginBadge tone="warning" label="builtin" />`
  - `<SourceBadge variant="cloned" />` → `<OriginBadge tone="info" label="cloned" />`
  - `<SourceBadge variant="user" />` → `<OriginBadge tone="neutral" label="user" />`
- [ ] 删除 [packages/app-web/src/features/assets-panel/_shared/SourceBadge.tsx](packages/app-web/src/features/assets-panel/_shared/SourceBadge.tsx)。

### A4 — 重写 _shared/Notice 为 @agentdash/ui Notice 的包装

- [ ] 改 [packages/app-web/src/features/assets-panel/_shared/Notice.tsx](packages/app-web/src/features/assets-panel/_shared/Notice.tsx)：保持 NoticeData / NoticeTone / NoticeProps 公共 API 不变；内部用 `@agentdash/ui` Notice，删除 emerald 字面色块。

### A5 — DesignSystemPage + spec 增量

- [ ] 在 [DesignSystemPage](packages/app-web/src/pages/DesignSystemPage.tsx) 增加 DetailPanel + CardMenu 预览段。
- [ ] 在 [.trellis/spec/frontend/design-language.md](.trellis/spec/frontend/design-language.md) §6 primitive 索引追加 DetailPanel / DetailSection / DetailMenu / DangerConfirmDialog / CardMenu 行。

### A 阶段验证

```bash
pnpm --filter @agentdash/ui run typecheck   # 如果 ui 包有独立 tsc
pnpm -r exec tsc --noEmit
pnpm --filter app-web exec eslint src 2>&1 | tail -3
```

期望：typecheck 通过；ESLint warning 数下降但仍 > 0（主要剩业务文件）。

## Phase B — 7 个 subagent 并行清扫

每个 subagent 提示词模板（每条独立分发，run_in_background=false 但并行）：

```
Active task: .trellis/tasks/05-19-design-language-migration

请阅读：
- prd.md（验收条件）
- design.md §1（颜色映射）+ §2（半径规则）

任务范围（仅本桶）：<bucket-name>
文件清单：<file-list>

只改 className，不动 props/逻辑/数据流。每改一个文件后跑：
  pnpm --filter app-web exec eslint <file> --no-fix
确认本文件 0 warning。

完成后回报：每文件改了多少 warning、是否有需要主会话决策的歧义。
```

### Bucket 分发

| Bucket | 调用 | 文件清单（src/ 相对） |
|---|---|---|
| SA-1 | trellis-implement | `pages/ProjectSettingsPage.tsx`, `pages/SettingsPage.tsx`, `pages/SessionPage.tsx`, `pages/StoryPage.tsx`, `pages/LoginPage.tsx` |
| SA-2 | trellis-implement | `features/project/project-agent-view.tsx`, `features/project/agent-preset-editor.tsx`, `features/project/project-selector.tsx`, `features/canvas-panel/ProjectCanvasManager.tsx`, `features/canvas-panel/CanvasSessionPanel.tsx`, `features/canvas-panel/CanvasFilesEditor.tsx`, `features/canvas-panel/CanvasRuntimePreview.tsx`, `features/canvas-panel/CanvasBindingsEditor.tsx` |
| SA-3 | trellis-implement | `features/workflow/**`, `features/agent/active-session-list.tsx`, `features/agent/agent-tab-view.tsx`, `features/task/task-drawer.tsx`, `features/task/agent-binding-fields.tsx` |
| SA-4 | trellis-implement | `features/session-context/hook-runtime-cards.tsx`, `features/session-context/surface-card.tsx`, `features/session/ui/**` |
| SA-5 | trellis-implement | `features/assets-panel/**`（不包含 _shared/SourceBadge/Notice/CardMenu — 已 Phase A 处理）, `features/workspace/workspace-list.tsx` |
| SA-6 | trellis-implement | `features/routine/routine-tab-view.tsx`, `features/workspace-panel/**`, `features/executor-selector/**`, `features/story/**` |
| SA-7 | trellis-implement | `features/vfs/**`, `components/layout/**`, `components/vfs-config-editor.tsx`, `components/ui/status-badge.tsx`, `App.tsx`, `features/file-reference/**`, `features/context-source/**` |

### B 阶段验证

```bash
pnpm --filter app-web exec eslint src --no-fix 2>&1 | tail -3
pnpm -r exec tsc --noEmit
pnpm --filter app-web test
```

期望：**0 warning**。如有残留：
- 主会话直接 inline 修复
- 或派一个 SA-8 兜底

## Phase C — 收口

- [ ] C1：再次确认 spec/frontend/design-language.md §6 完整，必要时补充新 primitive。
- [ ] C2：dev server 跑起来，请用户视觉验收 Settings / ProjectSettings / Story / Session / AssetPanel 主路径。
- [ ] C3：用户批准后 → 一次性 commit；然后 `/trellis:finish-work` 归档 + 写 journal。

## 风险与回滚

| 风险 | 概率 | 处置 |
|---|---|---|
| Phase A 改 import 路径漏改某文件，typecheck 红 | 低 | 用 grep 兜底，找 `from '@/components/ui/detail-panel'` 残留 |
| 某 subagent 把 `rounded-full` 该保留的也改了（StatusDot/Avatar），UI 变形 | 中 | design.md §2 例外清单已明确；视觉验收兜底 |
| `_shared/Notice` 重写后行为变（auto-dismiss 不工作） | 低 | 保留 useEffect 不动，只换视觉壳 |
| status-badge.tsx pill 形态从 `rounded-full` 改 `rounded-[8px]` 视觉不协调 | 中 | 验收时如不接受，加行内 eslint-disable 豁免 |
| 7 个 subagent 同时跑 OOM/超时 | 低 | 监控；必要时拆 2 波 |
