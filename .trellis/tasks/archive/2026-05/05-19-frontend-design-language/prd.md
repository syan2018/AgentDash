# 前端设计语言收敛与通用 UI 共用化

## Goal

明确 AgentDash 前端后续整体设计风格（容器层级、状态色、圆角刻度、密度），并把高频违反样式约定的字面色 / 字面圆角 / 多层嵌套盒子收敛到 `@agentdash/ui` 共用 primitive，固化为强制 spec 防回归。

最终目标是让"打开任意 dialog/inspector，视觉一致、密度可控、维护时只动 token"成为默认结果。

基线调研：[docs/reviews/design-language-audit-2026-05-19.md](docs/reviews/design-language-audit-2026-05-19.md)。

## Confirmed Facts

### 共用模块现状

- `@agentdash/ui` 包已存在并被 [packages/app-web/src/main.tsx](packages/app-web/src/main.tsx) 通过 `import '@agentdash/ui/styles.css'` 引入。
- 已建立 10 个 primitive：[Badge](packages/ui/src/primitives/Badge.tsx)、[Button](packages/ui/src/primitives/Button.tsx)、[Card](packages/ui/src/primitives/Card.tsx) + CardHeader、[CheckboxField](packages/ui/src/primitives/CheckboxField.tsx)、[EmptyState](packages/ui/src/primitives/EmptyState.tsx)、[Field](packages/ui/src/primitives/Field.tsx)、[Notice](packages/ui/src/primitives/Notice.tsx)、[Select](packages/ui/src/primitives/Select.tsx)、[TextInput](packages/ui/src/primitives/TextInput.tsx)、[Textarea](packages/ui/src/primitives/Textarea.tsx)。
- Badge primitive 已经支持 5 种语义 variant（`neutral / primary / success / warning / danger`），全部走 token，无字面色。
- Card primitive 默认 `rounded-[8px] border border-border bg-card p-4`，支持 `as=section/article/div/form`。
- 这套 primitive 由已归档任务 [05-14-tauri-web-style-component-unification](.trellis/tasks/archive/2026-05/05-14-tauri-web-style-component-unification/prd.md) 建立。
- `from '@agentdash/ui'` 在 packages 内的实际 import 数：app-web 业务组件 **0**；app-tauri 1（壳）；views 1（LocalRuntimeView）。**primitive 完全空跑**。

### 已有 token / utility class（[packages/ui/src/styles.css](packages/ui/src/styles.css)）

- 颜色：HSL 变量含 `background / foreground / card / popover / primary / secondary / muted / accent / destructive / warning / success / info` + 各 `-foreground`，dark 镜像齐备。
- 表单：`.agentdash-form-label`、`.agentdash-form-input/-select/-textarea`（**radius 12px / min-height 2.5rem**）。
- 按钮：`.agentdash-button-primary/-secondary/-danger`（**radius 10px**）。
- 其他：`.agentdash-file-pill`（radius 6.4px）、`.agentdash-panel-header-tag`（radius 8px）、`.agentdash-markdown` / `.agentdash-chat-markdown`（radius 7.2px）。

### 已有 spec

- [.trellis/spec/frontend/component-guidelines.md](.trellis/spec/frontend/component-guidelines.md) 仅约束 Tailwind v4 + cn + 颜色变量。
- **没有约束**：禁止字面色（`amber-500/30` 等）、圆角字面量、嵌套层数上限、强制使用 primitive。

### 高频问题清单（按文件密度，源自调研报告）

| 文件 | 问题 |
|------|------|
| [SkillCategoryPanel.tsx](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx) | color-literal × 7 + radius-mix × 5 + nested-card + row-box + pre-double-bg |
| [project-agent-view.tsx](packages/app-web/src/features/project/project-agent-view.tsx) | color-literal × 3（amber-400 vs amber-500 typo） |
| [task-drawer.tsx](packages/app-web/src/features/task/task-drawer.tsx) | nested-card 3 层（12 → 10 → full） |
| [PublishedBadge.tsx](packages/app-web/src/features/assets-panel/_shared/PublishedBadge.tsx) | violet 字面色三件套 |
| [MarketplaceAssetDrawer.tsx](packages/app-web/src/features/assets-panel/categories/MarketplaceAssetDrawer.tsx) | amber 字面色 + 400/500 混用 |
| [routine-tab-view.tsx](packages/app-web/src/features/routine/routine-tab-view.tsx) | EXEC_STATUS_STYLE 字面色映射 |
| [TabBar.tsx](packages/app-web/src/features/workspace-panel/TabBar.tsx) | radius-mix 7+4 |
| [vfs-browser.tsx](packages/app-web/src/features/vfs/vfs-browser.tsx) | radius-mix 6+8 |
| [story-detail-panels.tsx](packages/app-web/src/features/story/story-detail-panels.tsx) | nested-card + row-box |

StatusDot 高频出现位置（共 7+ 处）：[routine-tab-view.tsx:301](packages/app-web/src/features/routine/routine-tab-view.tsx#L301)、[active-session-list.tsx:77](packages/app-web/src/features/agent/active-session-list.tsx#L77)、[CommandExecutionCard.tsx:89](packages/app-web/src/features/session/ui/CommandExecutionCard.tsx#L89)、[SessionToolCallCard.tsx:150](packages/app-web/src/features/session/ui/SessionToolCallCard.tsx#L150)、[SessionChatView.tsx:717](packages/app-web/src/features/session/ui/SessionChatView.tsx#L717)、[workspace-list.tsx:89](packages/app-web/src/features/workspace/workspace-list.tsx#L89)、[terminal-tab.tsx:174](packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx#L174)、[vfs-browser.tsx:283](packages/app-web/src/features/vfs/vfs-browser.tsx#L283)。

### 调研期就地修复样本

- [SkillCategoryPanel.tsx:739-836](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx#L739-L836) `SkillVfsInspector`：4 层嵌套 → 1 层 sticky header + space-y。可作为后续 nested-card 修复样板。

## Brainstorm 决策记录

| Q | 议题 | 决议 |
|---|------|------|
| Q1 | Scope | **B**：spec + primitive 扩展 + 1-2 个高频文件示范迁移；其余高频违反点拆为后续子任务 |
| Q2 | 状态色 primitive | **B**：扩 Badge 加 `info / accent` variant；origin 单独 `OriginBadge` primitive，外部仅传枚举 |
| Q3 | 新 primitive 范围 | **InspectorRow + StatusDot + SectionTitle** 三个；不做 Toolbar / SegmentedControl |
| Q4 | 圆角刻度 | **B · 4 档**：xs=4 / sm=6 / md=8 / lg=12 |
| Q5 | input/button radius 同步 | **B**：input 12→8、button 10→8 一并落地，需附 visual review |
| Q6 | Lint 强制 | **B**：本任务加 warn 级 ESLint，限 `packages/app-web/src`，老文件 warning 后续子任务消化 |
| Q7 | 示范迁移文件 | **B**：[PublishedBadge.tsx](packages/app-web/src/features/assets-panel/_shared/PublishedBadge.tsx) (10 行) + [SkillCategoryPanel.tsx](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx) (1134 行) 全文迁移到 primitive |
| Q8 | 设计预览页 | **B**：独立路由 `/dev/design-system` + 全面预览 6 段（Tokens / Radius / Primitive 一览 / Surface depth / 旧新嵌套对比 / Form 综合）；并产环境都挂载，不进导航。提前在 S2 建骨架，后续每步 primitive 完成时补展示区。 |
| Q9 | 用色饱和度 | 用户在预览页视觉验收时反馈"部分用色希望克制一点"。决议：本任务一并把 `--primary / --destructive / --warning / --info` 的 HSL `S` 通道下调（92→74、84→68、93→84、78→64），dark 模式镜像同步。`--success` 也从 78→64 微降。`--ring` / `--info` 跟随 primary。 |
| Q10 | 按钮交互风格 | 用户在预览页视觉验收时反馈"喜欢空心、只用外框 + 字体颜色标识交互类别"。决议：Button primary / danger 改为空心（`border-{tone}/60 bg-transparent text-{tone}`，hover 时边框加深 + 微弱 `bg-{tone}/8` 染色）；secondary / ghost 维持已有空心形态。`.agentdash-button-primary/-danger` utility class 同步变空心，避免 primitive 与 utility 双路径不一致。 |
| Q11 | 表单 label 字体 | 用户在预览页视觉验收时反馈"表单字体设计希望整体优化"。决议：`.agentdash-form-label` 从 `11px uppercase letter-spacing-0.14em` 改为 `12px normal-case letter-spacing-0 line-height-1.25rem`；Field primitive 标签从 `font-semibold` → `font-medium`；CheckboxField 从 `font-semibold text-muted-foreground` → `font-medium text-foreground`（让选项文字成主体，不再被当 label）。 |

## Requirements

### R1 设计原则 spec

- 在 [.trellis/spec/frontend/](.trellis/spec/frontend/) 落地新 spec `design-language.md`，覆盖以下条款：
  - **Surface 层级**：定义 depth-0 / depth-1 / depth-2 三层；同一深度的视觉提示只允许 1 种（border / bg / shadow 三选一）；最大允许嵌套层数为 2。
  - **Radius 刻度**：xs=4 / sm=6 / md=8 / lg=12。每档对应组件示例（icon button / badge-pill / input-button-card-inspector / outer dialog）。
  - **状态色与品牌色分离**：状态走 Badge variant；origin 走 OriginBadge；禁止在业务组件中直接写颜色字面量。
  - **强制使用 primitive 的位置**：badge / dot / inspector row / section title / form field / button / card / dialog 入口，必须走 `@agentdash/ui` 而非自由组合 atom。
  - **Forbidden patterns** 列表（含 lint 规则原文）。
  - 与 [component-guidelines.md](.trellis/spec/frontend/component-guidelines.md) 的关系（互补，前者是组件实现规范，本 spec 是视觉表达规范）。

### R2 Token 调整

#### R2.1 Radius

- 修改 [packages/ui/src/styles.css](packages/ui/src/styles.css)：
  - `.agentdash-form-input/-select/-textarea` border-radius 由 `0.75rem` → `0.5rem`（12 → 8px）。
  - `.agentdash-button-primary/-secondary/-danger` border-radius 由 `0.625rem` → `0.5rem`（10 → 8px）。
  - 维持其余 utility class 不动；Markdown / pill / panel-tag 不在本任务范围。

#### R2.2 用色饱和度（Q9 落地）

- 修改 [packages/ui/src/styles.css](packages/ui/src/styles.css) 的 `:root` 与 `.dark`：
  - `--primary`：light `217 92% 50%` → `217 74% 54%`；dark `217 92% 60%` → `217 70% 64%`。
  - `--destructive`：light `0 84% 60%` → `0 68% 56%`；dark `0 72% 51%` → `0 60% 55%`。
  - `--warning`：light `45 93% 54%` → `38 84% 56%`（微偏橙、降饱和）；dark `45 93% 54%` → `38 78% 60%`。
  - `--success`：light `163 78% 43%` → `163 64% 42%`；dark 同步 `163 56% 48%`。
  - `--info`、`--ring` 跟随 primary。
- 不动：`--background` / `--foreground` / `--card` / `--secondary` / `--muted` / `--accent` / `--border` 等中性 token。

#### R2.3 表单 label 字体（Q11 落地）

- `.agentdash-form-label`：移除 `text-transform: uppercase`，`letter-spacing: 0.14em` → `0`，`font-size: 0.6875rem` → `0.75rem`，新增 `line-height: 1.25rem`。
- Field / CheckboxField primitive 字重对齐：`font-semibold` → `font-medium`；CheckboxField 文字色从 `text-muted-foreground` → `text-foreground`。

### R3 Primitive 扩展

- **Badge** ([packages/ui/src/primitives/Badge.tsx](packages/ui/src/primitives/Badge.tsx)) 扩展：
  - 新增 variant `info`（走 `--info` token）和 `accent`（走 `--accent` 或新增 token）。
  - radius 改为 `rounded-[6px]`（sm 档），与 spec 对齐。
- **OriginBadge** 新建：
  - props：`origin: 'builtin_seed' | 'user' | 'github' | 'clawhub' | 'skills_sh' | 'marketplace'` + 可选 `subText`（短 URL / 版本等）。
  - 调色集中封装；颜色靠 CSS 变量 / data attribute，绝不暴露 tailwind 字面色。
- **InspectorRow** 新建：
  - props：`label: string`、`value: ReactNode`、`mono?: boolean`、`tone?: 'default' | 'muted' | 'success' | 'warning' | 'danger'`。
  - 替换调研期 SkillCategoryPanel / vfs-browser / project-agent-view 中各自实现版本的形态。
- **StatusDot** 新建：
  - props：`tone: 'success' | 'warning' | 'danger' | 'info' | 'muted'`、`size?: 'sm'|'md'`、`pulse?: boolean`、`title?: string`。
  - 内部走 token，非字面色；本任务暂不强制在所有 7+ 处出现的位置全部替换（属下一阶段子任务）。
- **SectionTitle** 新建：
  - props：`title: string`、`badge?: ReactNode`、`actions?: ReactNode`、`subtitle?: string`。
  - 容纳 inspector header / panel header 的"标题 + tag + 行动按钮"复用形态。

### R4 Lint 防回归

- 在 [packages/app-web](packages/app-web) 的 ESLint 配置新增规则：
  - 禁字面色：`bg-(red|emerald|amber|sky|violet|orange|blue|green|rose|yellow|teal|cyan|fuchsia|pink|indigo)-\d+(/\d+)?` / `text-(...)-\d+` / `border-(...)-\d+`。
  - 禁字面圆角：`rounded-\[\d+px\]`。
  - 限制范围：`packages/app-web/src/**/*.{ts,tsx}`，level=`warn`，不 block CI。
  - 已知例外（如 markdown 渲染相关）通过 `// eslint-disable-next-line` 显式标注。

### R5 设计语言预览页

- 在 [packages/app-web/src/pages/](packages/app-web/src/pages/) 新建 `DesignSystemPage.tsx`，路由 `/dev/design-system`，挂载在 AuthGate 内但 `<WorkspaceLayout>` 之外（不依赖业务壳）。
- 页面分 6 段：
  1. **Tokens**：颜色 swatch（背景 / 文字 / 主色 / secondary / muted / destructive / warning / success / info / accent），含 light/dark 切换或同时展示。
  2. **Radius**：xs/sm/md/lg 四档视觉对比，标注每档建议组件。
  3. **Primitive 一览**：Badge 全部 7 variant、OriginBadge 6 origin、StatusDot 5 tone × 2 size + pulse、InspectorRow 多 tone 示例、SectionTitle 默认 / sticky / 含 actions、Button / TextInput / Textarea / Select / CheckboxField / Card / Notice / EmptyState / Field 全部展示。
  4. **Surface depth demo**：depth-0 / 1 / 2 三层合法形态 + 错误形态（border + bg 双叠 / 三层嵌套）反例。
  5. **嵌套对比**：截 SkillCategoryPanel 旧 inspector 样式 vs 新扁平化样式左右并列。
  6. **Form 综合**：模拟 Skill 编辑表单 + 任意 Dialog 表单，验证 input/button radius=8 后整体观感。
- 页面在并产环境都挂载，但不写入任何业务导航；只能手动 URL 访问。
- 后续此页面不属于业务功能，可随设计语言演进持续维护。

### R6 Button 交互风格（Q10 落地）

- [packages/ui/src/primitives/Button.tsx](packages/ui/src/primitives/Button.tsx)：
  - `primary`：`border-primary bg-primary text-primary-foreground` → `border-primary/60 bg-transparent text-primary hover:border-primary hover:bg-primary/8`
  - `danger`：同上换 destructive token
  - `secondary`：保留 `bg-background text-foreground`，hover 加 `border-foreground/30`
  - `ghost`：维持原状
- [packages/ui/src/styles.css](packages/ui/src/styles.css) 的 `.agentdash-button-primary/-danger` 同步空心化（border 60% opacity + bg transparent + text 同 tone + hover 强化），`.agentdash-button-secondary` hover 加边框深化。

### R7 示范迁移

（原 R5 示范迁移内容下移为 R6）

- 完整迁移 [PublishedBadge.tsx](packages/app-web/src/features/assets-panel/_shared/PublishedBadge.tsx)：内部用 `<Badge variant="accent">已发布 v{x}</Badge>`。
- 完整迁移 [SkillCategoryPanel.tsx](packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx)：
  - 7 处 color-literal 全部消除（origin → OriginBadge；explicit only / imported → Badge variant；emerald 保存按钮 → Button primitive 或 success-tinted）。
  - 5 处 radius-mix 字面值全部归 token（rounded-md/-sm 等）。
  - YAML meta panel 重构（参照已修的 SkillVfsInspector 模式）。
  - InspectorRow 替换内部自定义实现。
  - 完成后在 app-web 跑 ESLint 该文件零 warning。

## Acceptance Criteria

- [ ] **Spec**：[.trellis/spec/frontend/design-language.md](.trellis/spec/frontend/design-language.md) 落地，包含 Surface / Radius / 状态色 / Primitive 强制位 / Forbidden patterns 五节，并已写入 [.trellis/spec/frontend/index.md](.trellis/spec/frontend/index.md) 索引。
- [ ] **预览页**：`/dev/design-system` 路由可访问，6 段（Tokens / Radius / Primitive 一览 / Surface depth / 嵌套对比 / Form 综合）齐全，所有 primitive 在页面中可视化检查。
- [ ] **Token · radius**：`.agentdash-form-input/-select/-textarea` 与 `.agentdash-button-*` radius 落到 8px。
- [ ] **Token · 饱和度**：`--primary / --destructive / --warning / --success / --info` HSL S 通道按 R2.2 下调；视觉验收通过。
- [ ] **Token · 表单 label**：`.agentdash-form-label` 去 caps + 字号 12px + letter-spacing 0；Field / CheckboxField 字重对齐 medium。
- [ ] **Button 空心化**：primitive + utility 双路径 primary/danger 全部空心；视觉验收通过。
- [ ] **Primitive 扩展**：Badge variant `info / accent` 可用；新 primitive `OriginBadge / InspectorRow / StatusDot / SectionTitle` 在 [packages/ui/src/index.ts](packages/ui/src/index.ts) 导出，`pnpm --filter @agentdash/ui typecheck` 通过。
- [ ] **示范迁移**：
  - [ ] PublishedBadge.tsx 内部 0 字面色，使用 Badge primitive。
  - [ ] SkillCategoryPanel.tsx 内部 0 字面色、0 字面圆角；ESLint 该文件 warn=0。
  - [ ] SkillCategoryPanel.tsx 嵌套层数 ≤ 2（最深路径手工核对）。
- [ ] **Lint**：新增的字面色 / 字面圆角规则在 packages/app-web 内启用 warn 级；运行 `pnpm --filter app-web lint` 不报新增 error。
- [ ] **构建与类型**：`pnpm --filter @agentdash/ui typecheck`、`pnpm --filter app-web typecheck`、`pnpm --filter app-web build` 通过。
- [ ] **视觉回归**：Skill 编辑抽屉 / 表单 input 圆角变化、Skill 卡片 origin badge / 已发布 badge 颜色保持语义一致 — 由用户视觉验收（参照 [feedback_no_commit_until_approved](C:\Users\yihao.liao\.claude\projects\d--ABCTools-Dev-AgentDashboard\memory\feedback_no_commit_until_approved.md)）。
- [ ] **后续子任务规划**：在 [.trellis/tasks/](.trellis/tasks/) 下登记好待迁移的高频违反点列表（task-drawer / project-agent-view / MarketplaceAssetDrawer / routine-tab-view / TabBar / vfs-browser / story-detail-panels），不在本任务执行。

## Out of Scope

- 后端 / API / 数据库变更。
- 视觉重设计（color palette、字体、间距尺度的根本性更换）。
- 新增 dark / light 之外的主题。
- 已归档 05-14 任务覆盖的 Tauri/Web 入口统一议题。
- 全仓 sweep 替换字面色 / 字面圆角（拆为后续子任务）。
- 弃用 `.agentdash-form-* / .agentdash-button-*` utility class（本任务两条路径并存：utility 与 primitive）。
- Toolbar / SegmentedControl 等高级 primitive。
- StatusDot 的 7+ 处全量替换（本任务只交付 primitive，替换属后续子任务）。

## Notes

- 本任务为复杂任务，必须有 `design.md` + `implement.md` 才能 `task.py start`。
- 用户视觉验收前不得 commit（参照 feedback_no_commit_until_approved）。
- 本任务起步后若发现 input radius 8px 在密集表单中观感不佳，允许回滚到 10px 并在 spec 中标注 `md-form=10`，但需在 design.md 留下决策记录。
