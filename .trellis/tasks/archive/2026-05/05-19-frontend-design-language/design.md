# Design · 前端设计语言收敛与通用 UI 共用化

PRD: [prd.md](prd.md) · 调研: [docs/reviews/design-language-audit-2026-05-19.md](../../../docs/reviews/design-language-audit-2026-05-19.md)

## 1. 架构边界

```
┌──────────────────────────────────────────────────┐
│ .trellis/spec/frontend/design-language.md        │  ← 约束源（人读 + trellis-check）
└──────────────────────────────────────────────────┘
                 │ 引用
                 ▼
┌──────────────────────────────────────────────────┐
│ packages/ui                                       │
│   ├─ src/styles.css           (token utility)    │  ← 老 utility 路径
│   └─ src/primitives/*.tsx     (primitive 组件)   │  ← 新代码路径
└──────────────────────────────────────────────────┘
                 │ 消费
                 ▼
┌──────────────────────────────────────────────────┐
│ packages/app-web/src                              │
│   - 新代码强制 import @agentdash/ui primitive    │
│   - 老代码暂保留 .agentdash-* utility class      │
│   - ESLint warn 级反向闸阻断字面色 / 字面圆角    │
└──────────────────────────────────────────────────┘
```

**设计要点**：

- **三轨并行**：spec 描述 why、primitive 描述 how、lint 兜底 don't。
- **token utility 与 primitive 并存**：本任务不强制弃用 `.agentdash-form-*` / `.agentdash-button-*`，因为它们已经覆盖了 `<input>` / `<select>` / `<textarea>` / `<button>` 这些原始 element 的便捷使用，删掉会触发全仓改动。primitive 是新代码首选；老代码迁移在后续子任务里逐步替换。
- **token 调整与 primitive 同时发生**：input/button radius 从 12/10 → 8 在 utility class 与 primitive 内同步变更，保证两条路径输出一致。

## 2. Surface 层级 spec 细则

| Depth | 用途 | 视觉提示（**只能选一个**） |
|-------|------|---------------------------|
| 0 | 页面背景、layout shell | `bg-background`，无 border、无 shadow |
| 1 | Dialog / Panel / Card / Drawer | `bg-card border border-border rounded-md shadow-sm`（border + bg + 浅 shadow 这一组算"一个"组合标识） |
| 2 | Section / Inspector row / Subsection | **二选一**：`border-t border-border/40` **或** `bg-secondary/40 rounded-md` — 不允许同时叠 border + bg |
| 禁 | depth-3 | 任何在 depth-2 子区里再套圆角 + border + bg 的盒子 |

**例外条款**：

- `<pre>` 代码块允许独立的 `border + bg-secondary/40`，但所在 section 不能再有 bg；规则归到 `<pre>` 自己身上。
- 表单分组用 `<fieldset>` + `<legend>` + 间距分割（`space-y-3`），不算容器层级；属于"无视觉装饰的语义分组"。

**最大嵌套**：从 dialog 外壳算起到任意叶子 atom，最多 2 层 depth-1+ 的容器（外壳算 1，内部 section 算 2）。

## 3. Radius token 表

| Token | 值 | 适用 |
|-------|-----|-----|
| `xs` | 4px | icon-only button（如 SkillFileActionButton 的 7px → 4px、TabBar close button 的 4px 维持） |
| `sm` | 6px | badge / pill / chip / tag |
| `md` | 8px | input / select / textarea / button / card / dialog / inspector row / `<pre>` |
| `lg` | 12px | 大 outer dialog（仅 dialog shell 在视觉上需要更柔和的圆角时使用，谨慎） |

实现层面**不**新增 CSS 变量，只在 spec 文档里固化映射；primitive 内部直接写 `rounded-[Npx]`（保留字面值，但限定为 4 / 6 / 8 / 12 这 4 个 sentinel 值）。Lint 规则白名单这 4 个值，其余字面 px 全部 warn。

## 4. Primitive contract

### 4.1 Badge（扩展）

```ts
type BadgeVariant =
  | 'neutral' | 'primary'
  | 'success' | 'warning' | 'danger'
  | 'info'    // 新增：走 --info token，例 SUPERVISED 权限
  | 'accent'  // 新增：走 --accent / 自定义紫调，例 已发布
```

```css
/* variantClass 新增项（保持旧 5 项不动） */
info:   border-info/25 bg-info/10 text-info
accent: border-violet-500/25 bg-violet-500/10 text-violet-600 dark:text-violet-300
        ↑ accent 暂用 violet 字面色作为唯一保留例外（在 primitive 内部，外部 0 个字面色）
        后续若新增 --accent 主题变量，再切换；本任务不动主题。
```

radius：`rounded-[8px]` → `rounded-[6px]`（sm 档，与 spec 对齐）。
现有 `min-h-6 px-2 py-0.5 text-[11px]` 视觉不动。

### 4.2 OriginBadge（新建）

```ts
export type OriginKind =
  | 'builtin_seed' | 'user' | 'github' | 'clawhub' | 'skills_sh' | 'marketplace'

export interface OriginBadgeProps {
  origin: OriginKind
  subText?: string         // 短 URL / 版本，可选
  title?: string
  className?: string
}
```

内部维护一张映射：

```ts
const originStyle: Record<OriginKind, { label: string; tone: string }> = {
  builtin_seed: { label: 'builtin',     tone: 'neutral' },
  user:         { label: 'user',        tone: 'accent'  },
  github:       { label: 'github',      tone: 'info'    },
  clawhub:      { label: 'clawhub',     tone: 'success' },
  skills_sh:    { label: 'skills.sh',   tone: 'warning' },
  marketplace:  { label: 'marketplace', tone: 'success' },
}
```

实现：内部 `<Badge variant={...}>{label}{subText && `· ${subText}`}</Badge>`。复用 Badge primitive，不引入新 CSS。

### 4.3 InspectorRow（新建）

```ts
export interface InspectorRowProps {
  label: string
  value: ReactNode
  mono?: boolean
  tone?: 'default' | 'muted' | 'success' | 'warning' | 'danger'
  className?: string
}
```

```tsx
<div className={cn('space-y-1', className)}>
  <dt className="agentdash-form-label">{label}</dt>
  <dd className={cn(
    'break-words',
    tone === 'muted' ? 'text-muted-foreground'
    : tone === 'success' ? 'text-success'
    : tone === 'warning' ? 'text-warning'
    : tone === 'danger'  ? 'text-destructive'
    : 'text-foreground/85',
    mono && 'font-mono text-[11px]',
  )}>{value}</dd>
</div>
```

### 4.4 StatusDot（新建）

```ts
export interface StatusDotProps {
  tone: 'success' | 'warning' | 'danger' | 'info' | 'muted'
  size?: 'sm' | 'md'   // sm=1.5 (6px)、md=2 (8px)
  pulse?: boolean      // 配 absolute ping 动画外圈
  title?: string
  className?: string
}
```

实现走 token 色 (`bg-success / bg-warning / bg-destructive / bg-info / bg-muted-foreground/30`)，内部用 `inline-block rounded-full`。

### 4.5 SectionTitle（新建）

```ts
export interface SectionTitleProps {
  title: ReactNode
  subtitle?: ReactNode
  badge?: ReactNode    // 用于 InspectorTitleBar 那样在标题旁挂 SKILL.md tag
  actions?: ReactNode  // 右侧按钮区
  sticky?: boolean     // 顶部 sticky + 半透明背景
  className?: string
}
```

支持 SkillVfsInspector 的"sticky 顶栏 + 状态按钮"形态：

```tsx
<header className={cn(
  'flex items-center justify-between gap-3 px-4 py-3 border-b border-border/60',
  sticky && 'sticky top-0 z-10 bg-secondary/10 backdrop-blur supports-[backdrop-filter]:bg-secondary/30',
)}>
  <div className="min-w-0">
    <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
      {title}
    </p>
    {subtitle && (
      <p className="mt-0.5 truncate font-mono text-[11px] text-foreground/80">{subtitle}</p>
    )}
  </div>
  {(actions || badge) && (
    <div className="flex shrink-0 items-center gap-2">
      {badge}
      {actions}
    </div>
  )}
</header>
```

### 4.6 export 路径

[packages/ui/src/index.ts](packages/ui/src/index.ts) 增加：

```ts
export { OriginBadge } from './primitives/OriginBadge'
export type { OriginBadgeProps, OriginKind } from './primitives/OriginBadge'
export { InspectorRow } from './primitives/InspectorRow'
export type { InspectorRowProps } from './primitives/InspectorRow'
export { StatusDot } from './primitives/StatusDot'
export type { StatusDotProps } from './primitives/StatusDot'
export { SectionTitle } from './primitives/SectionTitle'
export type { SectionTitleProps } from './primitives/SectionTitle'
```

## 5. Token 调整范围

[packages/ui/src/styles.css](packages/ui/src/styles.css)：

| Selector | 旧 radius | 新 radius |
|----------|----------|-----------|
| `.agentdash-form-input` | `0.75rem` (12) | `0.5rem` (8) |
| `.agentdash-form-select` | `0.75rem` (12) | `0.5rem` (8) |
| `.agentdash-form-textarea` | `0.75rem` (12) | `0.5rem` (8) |
| `.agentdash-button-primary` | `0.625rem` (10) | `0.5rem` (8) |
| `.agentdash-button-secondary` | `0.625rem` (10) | `0.5rem` (8) |
| `.agentdash-button-danger` | `0.625rem` (10) | `0.5rem` (8) |

不动：`.agentdash-file-pill`（6.4px → 后续子任务调整为 6）、`.agentdash-panel-header-tag`（8px，已对齐）、`.agentdash-markdown` 系列（7.2px → 后续调整为 8）。

## 6. Lint 规则方案

ESLint flat config (`packages/app-web/eslint.config.js`) 增加规则：

```js
{
  files: ['src/**/*.{ts,tsx}'],
  rules: {
    'no-restricted-syntax': [
      'warn',
      {
        // 字面色 className
        selector: `JSXAttribute[name.name='className'] Literal[value=/\\b(bg|text|border)-(red|emerald|amber|sky|violet|orange|blue|green|rose|yellow|teal|cyan|fuchsia|pink|indigo)-\\d+(\\/\\d+)?\\b/]`,
        message:
          '禁止 Tailwind 字面色（如 amber-500/30）。状态色用 <Badge variant="..."> / <StatusDot tone="..."> / <OriginBadge>，参考 .trellis/spec/frontend/design-language.md',
      },
      {
        // 字面圆角，仅允许 4/6/8/12
        selector: `JSXAttribute[name.name='className'] Literal[value=/\\brounded-\\[(?!4px|6px|8px|12px)\\d+px\\]/]`,
        message:
          '禁止非 sentinel 字面圆角，仅允许 4 / 6 / 8 / 12 px。参考 .trellis/spec/frontend/design-language.md',
      },
    ],
  },
}
```

**风险点**：`no-restricted-syntax` + Literal value regex 在 ESLint 里实测可用，但只能 lint 静态字符串。模板字符串 / 变量插值不会被命中 — 这是已知限制，spec 中提示评审者注意；后续若需要更强的检查可引入 `eslint-plugin-tailwindcss`。

**例外机制**：业务代码确需保留字面色（如 markdown 代码块的渲染颜色映射）使用 `// eslint-disable-next-line no-restricted-syntax` 显式标注，并在 PR 中说明原因。

## 7. 示范迁移要点

### 7.1 PublishedBadge.tsx

```diff
-export function PublishedBadge({ version }: { version: string }) {
-  return (
-    <span
-      className="shrink-0 rounded-[6px] border border-violet-500/30 bg-violet-500/10 px-1.5 py-0.5 text-[10px] font-medium text-violet-700 dark:text-violet-300"
-      title="此资产已发布到资源市场"
-    >
-      已发布 v{version}
-    </span>
-  )
-}
+import { Badge } from '@agentdash/ui'
+
+export function PublishedBadge({ version }: { version: string }) {
+  return (
+    <Badge variant="accent" title="此资产已发布到资源市场" className="shrink-0">
+      已发布 v{version}
+    </Badge>
+  )
+}
```

### 7.2 SkillCategoryPanel.tsx 全文

按问题点分组处理：

| 段 | 行号区间 | 处理 |
|----|---------|------|
| ORIGIN_STYLE map + OriginBadge | [L363-L431](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L363-L431) | 删除整段 ORIGIN_STYLE / OriginBadge，改 import `OriginBadge` from @agentdash/ui，传 `origin={skill.source} subText={shortUrl}` |
| `marketplace` badge in OriginBadge fallback | [L401-L406](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L401-L406) | 合并到 OriginBadge 的 marketplace 分支 |
| Card 卡片 | [L489-L546](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L489-L546) | `<article>` 用 `<Card as="article">`，meta tags（file count / explicit only / imported）用 `<Badge variant="...">` |
| YAML meta panel | [L848-L887](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L848-L887) | section 外壳改 `<fieldset>`，去 border + bg；frontmatter `<pre>` 单层 border md radius |
| SkillVfsInspector header | [L739-L836](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L739-L836) | 用 `<SectionTitle sticky title="YAML meta" subtitle="SKILL.md" actions={保存按钮} />` |
| InspectorRow helper | [L820-L827](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L820-L827) | 删除本地 helper，import `InspectorRow` from @agentdash/ui |
| 保存 meta 按钮 emerald | [L774-L781](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L774-L781) | 用 `<Button variant="success" size="sm">` 或 Badge tonal 样式 — 待 Button primitive 是否已有 success variant 决定（如无，用 primary 的 outline 替代） |
| extra files action button | [L1015-L1044](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L1015-L1044) | 暂保留本地实现（属 icon button 群，迁移成本高，留子任务） |

迁移完成后该文件 ESLint warn 数 = 0，但允许保留 1-2 个 `// eslint-disable-next-line` 标注（如不可避免的字面色）。

## 8. 兼容性 / 回滚

- **input radius 8 不被接受**：spec 文档保留 fallback 条款，回滚到 `0.625rem` (10px)，并在 spec 标注 `md-form=10` 例外。Token 文件保留 git history 单 commit 化，便于 `git revert`。
- **Lint warn 噪音过大**：调整 lint 规则的 files glob，仅限 `src/features/**` 不覆盖 `src/components/**`，待存量消化后再扩。
- **primitive contract 误差**：每个 primitive 单独 commit；如 InspectorRow / StatusDot 设计与实际使用不符，可在示范迁移阶段通过 props 调整修补。

## 9. 验证策略

| 项 | 命令 / 步骤 |
|----|------------|
| Type | `pnpm --filter @agentdash/ui typecheck` |
| Type | `pnpm --filter app-web typecheck` |
| Lint | `pnpm --filter app-web lint` (新规则 warn=N，已知文件外 0；SkillCategoryPanel.tsx warn=0) |
| Build | `pnpm --filter app-web build` |
| 视觉 | 用户手工核对：Skill 列表卡 origin badge / 已发布 badge / 编辑抽屉 inspector / form input 圆角 |
| Spec | `cat .trellis/spec/frontend/design-language.md` 内含 5 节 |

## 10. 风险

| 风险 | 缓解 |
|------|------|
| input radius 8 视觉变窄 | 提供回滚 commit；视觉验收前不 push |
| `no-restricted-syntax` 漏掉模板字符串 | spec 明确"模板字符串靠 review"；trellis-check 时关注 |
| Badge accent variant 内部仍含 violet 字面色 | 单点保留，下任务考虑加 `--accent` 主题变量后再清理 |
| OriginBadge 枚举漏 | `OriginKind` 类型与 `SkillAssetDto.source` 联合类型对齐；新增 origin 时 type narrowing 失败即编译报错 |

## 12. 设计语言预览页

**目标**：把 spec、token、所有 primitive、Surface 层级、嵌套对比、表单综合在一个 React 页面里集中展示，作为本任务及后续设计调整的可视化验收基准。

### 12.1 路由与挂载

- 文件：`packages/app-web/src/pages/DesignSystemPage.tsx`
- 路由：`/dev/design-system`
- 位置：在 `App.tsx` 的 `<Routes>` 内、`WorkspaceLayout` 路由组**之外**（与 `<Route element={<WorkspaceLayout />}>` 平级），不依赖业务导航壳，但仍在 `AuthGate` 内（保持登录态一致）。
- 不挂导航：业务导航不增加链接；仅手动 URL 访问。
- 并产环境都挂载（不做条件编译）。

```tsx
<Routes>
  <Route path="/dev/design-system" element={<DesignSystemPage />} />
  <Route element={<WorkspaceLayout />}>
    {/* 现有业务路由不动 */}
  </Route>
</Routes>
```

### 12.2 页面结构

页面自带极简 layout（`bg-background min-h-screen p-6 max-w-6xl mx-auto`），顶部固定 anchor 导航跳到 6 段。

#### Section 1 · Tokens

- 颜色 swatch 网格：`background / foreground / card / popover / primary / secondary / muted / accent / destructive / warning / success / info`，每色显示色块 + token 名 + HSL 字符串。
- 含一个手动 dark mode toggle（局部为页面加 `class="dark"`），不影响全局 theme store。

#### Section 2 · Radius

- 4 个 100×60 矩形按 xs=4 / sm=6 / md=8 / lg=12 排列，每个标注适用场景（icon button / badge / input·button·card / outer dialog）。
- 加一行小字："4 / 6 / 8 / 12 是 sentinel 值，其他字面圆角会触发 lint warn。"

#### Section 3 · Primitive 一览

按字母序展示，每个 primitive 一个子区：

| Primitive | 展示内容 |
|-----------|---------|
| Badge | 7 variant × 短文字（"已发布 v1.2.3" 等真实场景） |
| Button | 3 variant × 2 size + disabled 态 |
| Card | as=section / article / div / form 各一例 + CardHeader actions |
| CheckboxField | 标签 + 描述 + checked / disabled |
| EmptyState | 默认 + 含 action 按钮 |
| Field | label + 各种 children（input / select / textarea） |
| InspectorRow | mono / 普通 / 各 tone（5 种） |
| Notice | success / warning / danger 三 tone + dismissable |
| OriginBadge | 6 origin × 是否带 subText |
| SectionTitle | 默认 / sticky / 含 actions / 含 badge |
| Select | 默认 + disabled |
| StatusDot | 5 tone × 2 size + pulse 动画 |
| TextInput | 默认 / placeholder / disabled / 错误态 |
| Textarea | 默认 / autosize / 长内容 |

每个 primitive 区底部带 import 提示："`import { Badge } from '@agentdash/ui'`"，方便对照实现。

#### Section 4 · Surface depth demo

3 个并排示例：

- **depth-1 合法**：`bg-card border rounded-md` 单层壳
- **depth-2 合法**：壳内仅用 `border-t` 分隔的 section
- **错误反例**：壳内嵌套 `border + bg + rounded` 子卡片（标红 ✗，附说明"违反二选一规则"）

#### Section 5 · 嵌套对比

左右两栏并列：

- **左**：截取 SkillVfsInspector 旧版风格（4 层嵌套）— 重建一份 read-only mock，不依赖 VFS surface
- **右**：本任务交付的扁平化版本（用新 SectionTitle + InspectorRow）

#### Section 6 · Form 综合

- 模拟 Skill 编辑表单：display_name / key / description / disable-model-invocation / 一段 SKILL.md textarea / 提交按钮
- Dialog 嵌套示例：触发按钮 → 弹出 dialog（Card depth-1）→ 内部 form
- 重点验收 input/button radius=8 后的整体观感

### 12.3 实现细节

- 单文件实现，不拆子组件，便于一眼看完页面（预计 600-900 行 JSX）。
- 不接 store / 不接 API；所有数据为本地 mock。
- 页面自身不进入 lint warn 名单（顶部加 file-level eslint-disable-next-line 是允许的，但展示用的 mock 数据本来就不会触发）。
- `import` 全部从 `@agentdash/ui`，作为 primitive 的 reference 用法。

### 12.4 维护

- 本页面不归属任何 feature，由 `.trellis/spec/frontend/design-language.md` 引用作为 visual reference。
- 后续设计调整（新 primitive、新 token）必须同步更新此页面。这条入 spec。

## 11. 后续子任务（不在本任务范围）

完成本任务后登记到 `.trellis/tasks/` 的迁移子任务（按优先级）：

1. project-agent-view.tsx color-literal × 3 → Badge variant
2. task-drawer.tsx nested-card 3 层 → Card + InspectorRow
3. MarketplaceAssetDrawer.tsx amber-400/500 typo → Badge
4. routine-tab-view.tsx EXEC_STATUS_STYLE → Badge variant 映射
5. StatusDot 在 7+ 高频文件批量替换
6. TabBar.tsx / vfs-browser.tsx / story-detail-panels.tsx 收尾
7. 评估 `.agentdash-form-* / .agentdash-button-*` utility class 是否弃用并全量切到 primitive
