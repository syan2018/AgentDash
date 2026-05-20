# 设计：前端设计语言批量迁移

## 1. 颜色字面色 → 语义 token 映射

业务文件里出现的字面色按以下规则替换。**primitive 内部**允许 violet（已在 OriginBadge / Badge.accent 局部隔离）；**业务文件**不允许任何 Tailwind 调色板字面色。

| 字面色家族 | 主用途 | 目标 token | 备注 |
|---|---|---|---|
| violet / purple / fuchsia | 标记发布、marketplace、accent | `accent`（Badge variant=accent / OriginBadge tone=accent） | 不要直写 `bg-violet-500/10`，用 `Badge accent` |
| sky / blue / indigo / cyan | 信息提示、链接强调 | `info` / `primary` | 链接走 `primary`；中性提示走 `info` |
| emerald / green / lime / teal | 完成、成功、保存 | `success` | 含按钮主色 |
| amber / orange / yellow | 警告、待审、注意 | `warning` | |
| rose / red / pink | 危险、错误 | `destructive` | |
| zinc / slate / gray / neutral / stone | 中性表面/文字 | `muted` / `secondary` / `border` | 按 surface depth 选择 |

**透明度**：`bg-X-500/10` → `bg-success/10`（同 token 取自身），`text-X-700 dark:text-X-300` → `text-success`（token 已自动适配明暗）。

## 2. 半径标准化

| 原值 | 替换 | 例外 |
|---|---|---|
| `rounded-full` | `rounded-[8px]` 或 `rounded-[12px]` | 圆形指示点（StatusDot/dot）/ 头像 / 圆形头像按钮：保留 |
| `rounded-xl` | `rounded-[12px]` | 大容器（Dialog 顶层）走 12 |
| `rounded-2xl` / `rounded-3xl` | `rounded-[12px]` | |
| `rounded-[5px]/[7px]/[9px]/[10px]/[14px]/[16px]/[18px]` | 就近到 4/6/8/12 | 一般 5→6, 7→8, 9→8, 10→8, 14→12, 16→12 |

`rounded-[20px]` 这种夸张值如果还存在，按 12 处理。

## 3. Primitive 抽取与上提（Phase A，主会话 inline 处理）

### 3.1 DetailPanel / DetailSection / DetailMenu / DangerConfirmDialog

来源：[packages/app-web/src/components/ui/detail-panel.tsx](packages/app-web/src/components/ui/detail-panel.tsx)
使用：12 个文件 81 处引用。

动作：
- 将 4 个组件迁到 `packages/ui/src/primitives/DetailPanel.tsx`（共用一文件，除非超过 200 行再拆）。
- 修内部违规：`rounded-[10px]` → 8，`rounded-[16px]` → 12。
- 业务侧改 import：`@/components/ui/detail-panel` → `@agentdash/ui`。
- `components/ui/detail-panel.tsx` 删除。
- 在 [DesignSystemPage](packages/app-web/src/pages/DesignSystemPage.tsx) 增加预览。
- 在 spec §6 登记。

### 3.2 CardMenu

来源：[packages/app-web/src/features/assets-panel/_shared/CardMenu.tsx](packages/app-web/src/features/assets-panel/_shared/CardMenu.tsx)
使用：4 个 panel 文件。

动作：
- 迁到 `packages/ui/src/primitives/CardMenu.tsx`。
- 修内部 `bg-amber-500/15 text-amber-600` 字面色 → `bg-warning/15 text-warning`；`rounded-[10px]` → 8。
- 调用方 import 改为 `@agentdash/ui`。

### 3.3 SourceBadge → 合并到 OriginBadge

私域 `SourceBadge`（marketplace/builtin/cloned/user）只剩 2 个使用方（McpPreset、Workflow panels）。直接调用 OriginBadge：

| SourceBadge variant | 替换 |
|---|---|
| `marketplace` | `<OriginBadge tone="success" label="marketplace" />` |
| `builtin` | `<OriginBadge tone="warning" label="builtin" />` |
| `cloned` | `<OriginBadge tone="info" label="cloned" />` |
| `user` | `<OriginBadge tone="neutral" label="user" />` |

迁移完成后删除 `_shared/SourceBadge.tsx`。

### 3.4 _shared/Notice → 重构为 @agentdash/ui Notice 的 dismissable 包装

`@agentdash/ui` Notice 是纯展示，缺少 auto-dismiss + close button。两种方案：

**选定方案**：保留 `_shared/Notice.tsx`，但内部用 `@agentdash/ui` Notice + 自带 close button。这样 emerald 字面色消失，行为不变。

```tsx
import { Notice as UiNotice, type NoticeTone as UiNoticeTone } from '@agentdash/ui'

const TONE_MAP: Record<NoticeTone, UiNoticeTone> = {
  success: 'success',
  danger: 'danger',
}

export function Notice({ notice, onDismiss, autoHideMs = 4000 }: NoticeProps) {
  // useEffect timer 不变
  if (!notice) return null
  return (
    <UiNotice
      tone={TONE_MAP[notice.tone]}
      role={notice.tone === 'danger' ? 'alert' : 'status'}
      className="flex items-center justify-between"
    >
      <p className="text-xs">{notice.message}</p>
      <button ... className="ml-2 text-xs opacity-70 hover:opacity-100">×</button>
    </UiNotice>
  )
}
```

不上提到 `@agentdash/ui` —— auto-dismiss 是业务交互而非视觉 primitive。

### 3.5 status-badge.tsx 就地修复（不上提）

`Story/Task/Priority/Type Badge` 是领域 badge，绑定 `StoryStatus / TaskStatus` enum。这些类型在 app-web 内，与 `@agentdash/ui` 解耦原则冲突。**就地修复**：
- `rounded-full` → `rounded-[8px]`（这些是 pill 形态，但走 8px 仍然 OK；如果觉得视觉差异大，保留 `rounded-full` 并加 eslint-disable 行注释也可，但优先方案是 8px）
- 颜色已经是 token，无需改。

> 决策点：如果用户视觉验收时觉得 `rounded-full` 的 pill 形态对状态徽标更自然，回退到 `rounded-full` 但加 `// eslint-disable-next-line no-restricted-syntax` 行内豁免（pill 是状态徽标的合理形态）。先按 8px 跑，验收时调。

## 4. 业务文件迁移分桶（Phase B，并行 subagent）

每个 bucket 由一个 `trellis-implement` subagent 处理。每个 subagent 拿到：
1. 本 design.md（作为 mapping 参考）
2. 任务的 prd.md
3. 自己的文件清单
4. 一行通用指令：**只改 className，不动 props/逻辑/数据流**

| Bucket | 模块 | warn | 关键文件 |
|---|---|---|---|
| **SA-1 pages** | pages 全量 | 79 | ProjectSettingsPage 32 / SettingsPage 29 / SessionPage 9 / StoryPage 7 / LoginPage 2 |
| **SA-2 project + canvas** | features/project + features/canvas-panel | 52 | project-agent-view 14 / agent-preset-editor 14 / project-selector 5 / ProjectCanvasManager 13 + Canvas* 6 |
| **SA-3 workflow + agent + task** | features/workflow + features/agent + features/task | 43 | workflow/* (32) / active-session-list 6 / agent-tab-view 1 / task-drawer 2 / agent-binding-fields 2 |
| **SA-4 session-context + session** | features/session-context + features/session | 56 | hook-runtime-cards 27 / surface-card 1 / Session* 28 |
| **SA-5 assets-panel + workspace** | features/assets-panel + features/workspace | 39 | assets-panel/* (22, 不包括已 Phase A 处理的 _shared) / workspace-list 17 |
| **SA-6 routine + workspace-panel + executor-selector + story** | 上述四个 features | 50 | routine-tab-view 15 / ContextOverviewTab 11 + AddressBar 1 + AddTabMenu 1 / ExecutorSelector 11 / story/* 11 |
| **SA-7 vfs + components + 杂项** | features/vfs + components/* + App.tsx + 小 features | 37 | vfs-browser 7 + vfs-code-editor 3 / workspace-layout 9 / vfs-config-editor 8 / status-badge 2 + detail-panel 3（实际由 Phase A 删除） / App 3 / file-reference 1 / context-source 1 |

注：components/ui/detail-panel.tsx 在 Phase A 被删除。SA-7 不应再处理它。

## 5. 兼容与回滚

- 每个 subagent 在自己的文件范围内编辑；冲突可能出现在 Phase A 修改的 import 路径同时被 Phase B 改 className（同一文件并发编辑）。
- **缓解**：Phase A 优先完成（主会话 inline）→ commit 一次稳定基线（不 push） → 再派发 Phase B。Phase B 内部各 bucket 文件不重叠。
- **回滚**：每个 bucket 独立 commit；如果某 bucket 引入回归，单点 revert 即可。

## 6. 操作开关

- **是否升级 lint 到 error 级**：本任务保持 warn 级。验证 0 warning 通过后，未来可由用户手动升 error，不在本任务范围。
- **DesignSystemPage 同步**：Phase A 必须更新（新增 DetailPanel + CardMenu 预览）；Phase B 不需要动 DesignSystemPage。
- **spec 同步**：[.trellis/spec/frontend/design-language.md](.trellis/spec/frontend/design-language.md) §6 在 Phase A 完成后增 DetailPanel/CardMenu 行。
