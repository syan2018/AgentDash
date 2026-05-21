# 前端设计语言批量迁移与 primitive 抽取

## Goal

将 `packages/app-web` 中所有 `no-restricted-syntax` ESLint warning 清零，并把私域复用的 UI 元素上提到 `@agentdash/ui` primitive，形成"违规即红灯"的工程闭环。

上一轮（`05-19-frontend-design-language`）建立了规则与 12 个 primitive，但故意保留了 356 条 warning 作为渐进迁移信号。本任务一次性收口。

## Confirmed facts

- ESLint 规则已在 [packages/app-web/eslint.config.js](packages/app-web/eslint.config.js) 启用，warn 级 `no-restricted-syntax` 覆盖三类违规：
  1. Tailwind 调色板字面色（violet/sky/emerald/orange/amber/rose/red 等 18 种）
  2. `rounded-[Xpx]` 非 4/6/8/12 档位
  3. `rounded-xl|2xl|3xl|full`（仅 StatusDot/Avatar 等极少场景豁免）
- 现状审计（详见 [research/eslint-bucketize.py](research/eslint-bucketize.py) 输出）：

  | 模块 | warn |
  |---|---|
  | pages | 79 |
  | features/project | 33 |
  | features/workflow | 32 |
  | features/session-context | 28 |
  | features/session | 28 |
  | features/assets-panel | 22 |
  | features/canvas-panel | 19 |
  | features/workspace（workspace-list 单文件） | 17 |
  | features/routine（routine-tab-view 单文件） | 15 |
  | features/workspace-panel | 13 |
  | features/executor-selector（ExecutorSelector 单文件） | 11 |
  | features/story | 11 |
  | features/vfs | 10 |
  | components/layout | 9 |
  | components/vfs-config-editor | 8 |
  | features/agent | 7 |
  | components/ui | 5 |
  | features/task | 4 |
  | App.tsx + 杂项 | 5 |
  | **TOTAL** | **356** |

- 已存在的 12 个 primitive：`Button / Badge / Card / EmptyState / Field / CheckboxField / TextInput / Textarea / Select / Notice / OriginBadge / InspectorRow / StatusDot / SectionTitle`（[packages/ui/src/primitives/](packages/ui/src/primitives/)）。
- 已发现的"私域 primitive"候选（应上提到 `@agentdash/ui`）：
  - `features/assets-panel/_shared/CardMenu.tsx`（多处复用的卡片菜单）
  - `features/assets-panel/_shared/SourceBadge.tsx`（与 OriginBadge 概念重叠，需合并/区分）
  - `features/assets-panel/_shared/Notice.tsx`（已有 `@agentdash/ui` Notice，需迁移调用方后删除私域版本）
  - `components/ui/status-badge.tsx`（应上提）
  - `components/ui/detail-panel.tsx`（待评估）

## Requirements

### R1 — Lint 清零

- `pnpm --filter app-web exec eslint src` 输出 **0 errors / 0 warnings**。
- 规则保持 warn 级（不升级到 error），但实际 warning 数为 0。

### R2 — 字面色迁移到语义 token

按以下映射收敛（详见 design.md §1）：

- 紫/violet → `accent` 或 `primary`（依语义而定，发布/marketplace 走 accent）
- 蓝/sky/blue → `info` 或 `primary`
- 绿/emerald/green → `success`
- 橙/amber/orange → `warning`
- 红/rose/red → `destructive`
- 中性灰 → `muted` / `secondary` / `border` / `card`

不允许 `bg-violet-500/10` 这种字面 + 透明度的写法**直接出现在业务文件**，必须封装在 primitive 内或走 token。

### R3 — 半径标准化

- `rounded-full` 仅在 StatusDot / Avatar / 圆点指示器中保留；其余替换为 `rounded-[8px]` 或 `rounded-[12px]`。
- 非 4/6/8/12 档位的 `rounded-[Xpx]` 统一规整到最接近的标准档。

### R4 — Primitive 抽取与上提

- 把"私域 primitive"候选评估后上提到 `packages/ui/src/primitives/`，并：
  1. 在 [packages/ui/src/index.ts](packages/ui/src/index.ts) 导出
  2. 在 [packages/app-web/src/pages/DesignSystemPage.tsx](packages/app-web/src/pages/DesignSystemPage.tsx) 增加预览
  3. 在 [.trellis/spec/frontend/design-language.md](.trellis/spec/frontend/design-language.md) §6 登记
- 调用方迁移到新 primitive，私域旧实现删除。

### R5 — 迁移过程不破坏行为

- typecheck（`pnpm -r exec tsc --noEmit`）通过
- vitest（`pnpm --filter app-web test`）通过
- DesignSystemPage 视觉与上一版一致或更好
- 关键页面（Settings / ProjectSettings / Story / SessionPage）肉眼回归（用户视觉验收）

## Acceptance Criteria

1. **Lint**：`pnpm --filter app-web exec eslint src` → 0 warnings
2. **类型**：`pnpm -r exec tsc --noEmit` 通过
3. **测试**：`pnpm --filter app-web test` 通过
4. **Primitive 一致性**：
   - `_shared/SourceBadge`、`components/ui/status-badge` 评估后合并/上提
   - `_shared/CardMenu`、`components/ui/detail-panel` 上提（如确有复用）
   - `_shared/Notice` 调用方迁到 `@agentdash/ui` Notice，私域版本删除
5. **文档**：[.trellis/spec/frontend/design-language.md](.trellis/spec/frontend/design-language.md) §6 primitive 索引同步更新
6. **视觉验收**：用户在 Settings / ProjectSettings / Story / Session / Asset Panel 主路径肉眼通过

## Out of scope

- 暗色模式独立色相调优（沿用现状）
- 字体/间距 token 重构（仅碰被违规规则点中的样式）
- 行为/交互调整（本任务**只**改样式 className）
- 视觉验收前的 `git commit`

## Open questions

无 —— 范围 A 已锁定（用户裁定 0 warning），并行 subagent 分工已确认。
