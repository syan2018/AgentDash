# Story 体验对齐 Multica：看板/创建/详情交互优化

## Goal

对照 `references/multica` 的 issue 面板设计，对我们 Story 工作台（看板、创建流程、详情面板）做一轮系统性的交互升级，让常见操作更快、更键盘友好、更一致。

## Background

最近一次提交 `f16d60a8 feat(story): polish Story 工作台与详情交互` 已对齐了卡片/徽章的视觉骨架，本任务在此基础上聚焦 **交互层面** 的差距收敛。

调研对比详见同目录 `research/comparison.md`（实现期生成）。核心差距（按价值排序）：

1. **快速编辑能力弱** — 状态/优先级/类型必须进入 detail edit mode 才能改；Multica 用 popover picker 卡片右键即可。
2. **创建流程重** — 当前 drawer 多分区表单；Multica 的 column "+" 直接打开轻量 modal，且支持「保持打开」连续创建。
3. **键盘几乎缺席** — 没有 Cmd+K 跳转、Cmd+N 新建；Multica picker 内有 ↑/↓/Enter + IME 安全。
4. **看板没有搜索/筛选** — 必须切到 list 才能找；Multica 的 header 控件对两种视图共用。
5. **没有批量操作** — Multica 有底部 batch toolbar（多选 + 批改状态/优先级 + 批删）。
6. **状态控件不统一** — `StoryStatusActions`（按钮）与 Properties 编辑（dropdown）affordance 冲突。
7. **Context 选择体感差** — 全是 checkbox，无全选/反选，Story 多了就累。
8. **缺微交互细节** — Tooltip（P0/FEAT 等）、空列提示、卡片 hover 优先级/操作菜单等。

## Scope（MVP）

本轮覆盖以下条目；其它降级到 [Out of Scope](#out-of-scope) 留给后续。

### S1. 看板/列表共享筛选与搜索
- 把 `story-list-view.tsx` 顶部 toolbar（搜索、scope、status/priority/type 筛选、sort、clear）从 list 视图独占抽到上层，board 视图也消费同一份 state。
- Sort 在 board 视图体现为 **列内排序**（保留现有 priority 权重 + updated_at fallback）。

### S2. 行内 popover 快速编辑（卡片 & 列表行）
- 卡片/行的 status / priority / type 徽章变成可点击的 popover picker；不再需要进入 detail。
- Picker 通用化：键盘 ↑/↓/Enter，IME 安全，悬停高亮，单结果自动选中（参照 Multica `property-picker.tsx`）。
- Detail 面板内的 status/priority/type 也复用同一 picker，统一来源；移除 Properties 单独的 Select。

### S3. 状态控件统一
- 删除/合并 `StoryStatusActions`（"标记就绪""开始执行" 这种命名按钮）；改用 status popover + Inspector 上方的 **主操作按钮**（next-step CTA），按当前 status 动态切换文案。
- 文案与 picker 列表同源，避免漂移。

### S4. 快速创建（行内 + 保持打开）
- Board 列头 "+" 改为 **inline quick-add**（点击后列顶部出现一个最小输入框：标题 + Enter 提交，Esc 取消）。
- 完整表单仍保留为 "Open full form" 入口（drawer）。
- Drawer 里加 `Keep open` 开关（提交后清空 + 保持开启），引导连续录入；状态预填来自打开时的列上下文。

### S5. 键盘快捷键
- 全局：`Cmd/Ctrl+N` 在当前视图新建 story（沿用列表/列上下文）；`Cmd/Ctrl+K` 打开 quick-jump（fuzzy 匹配 title / story key）。
- 卡片聚焦时：`E` 进入 detail，`X` toggle done，`P` 打开 priority picker。
- 范围限定：仅在 Story 路由下挂载；输入框/编辑器 focus 时屏蔽。

### S6. 多选 & 批量操作（先做最小版）
- List 视图行 hover/选中时在 priority 图标位置出现 checkbox（参照 Multica `list-row.tsx`）。
- 选中 ≥1 时底部出现 floating toolbar：批改 status / priority；批删（带确认）。
- 不在本轮做：Board 视图多选、批改 type/tags。

### S7. Context 选择体验
- `create-task-panel.tsx` 的 context checkbox 列表加 **全选 / 反选 / 清空** 三个 helper。
- 列表项右上角加 kind 图标 hover tooltip（FILE/TEXT/SNAP/HTTP/MCP/REF）。
- Story 中 context 数量阈值（>10）时折叠成可展开分组。

### S8. 视觉/微交互打磨
- 给 P0/P1/P2/P3 与 FEAT/BUG 等徽章加 hover tooltip 解释含义。
- Card 拖拽态参照 Multica：`rotate-2 scale-105 + shadow-lg + cursor-grabbing` —— 当前已有，确认对齐即可。
- 空列提示由"无故事"改为可点击的 "Create in this column"（已部分支持，统一文案）。
- Description 单行截断改为 `line-clamp-2`，并在 hover 时延迟 400ms 显示完整内容 popover（仅 board 视图）。

## Out of Scope（明确不做，避免 scope creep）

- 看板/列表虚拟化与服务端分页（Story 量级当前不需要）。
- 双模创建（Agent vs Manual）—— Multica 这块是核心特色，但需要 Agent 执行链路配合，单独立项。
- 活动时间线 / 操作历史 / who-changed-what。
- 上下文菜单（右键卡片）—— 等批量与 picker 落地后再评估。
- 服务端搜索 / 跨 project 全局搜索。
- 多人指派（当前 Story 无 assignee 字段）。

## Acceptance Criteria

- [ ] **S1**：在 board 视图也能搜索 / 筛选 / 排序，与 list 视图状态联动；切换视图时筛选保持。
- [ ] **S2**：在 board 卡片或 list 行上点击 status / priority / type 徽章即弹出 picker；键盘可操作；变更立即反映。
- [ ] **S3**：detail 面板内只有一处 status 入口，与卡片 picker 同源；移除分裂的 status 按钮组。
- [ ] **S4**：board 列头 "+" 触发 inline quick-add，标题非空 Enter 即创建；完整表单 drawer 内有 "Keep open" 开关，开启时连续创建不关闭。
- [ ] **S5**：在 Story 路由下，`Cmd/Ctrl+N` 创建、`Cmd/Ctrl+K` 跳转、`E/X/P` 卡片快捷键可用；输入态下不触发。
- [ ] **S6**：List 视图支持 hover/选中显示 checkbox；选中后底部 toolbar 提供批改状态/优先级/批删；空选时 toolbar 隐藏。
- [ ] **S7**：Context 选择有全选/反选/清空；kind 图标有 hover tooltip。
- [ ] **S8**：徽章 tooltip、card hover 描述 popover、空列 CTA 三项落地，文案与现有 i18n 体系一致（如有）。
- [ ] 不引入 a11y 回归：所有 picker / popover 支持 Esc 关闭、tab 进入、focus trap 合理。
- [ ] `pnpm -F app-web typecheck` 与 `pnpm -F app-web lint` 通过；现有单测不退化。
- [ ] 用户在浏览器中按"创建 → 拖动 → picker 改属性 → 多选批改 → 快捷键跳转"全链路走一遍验收通过。

## Constraints / Non-functional

- 不破坏现有 store 接口与后端协议；所有改动收敛在 `packages/app-web/src/features/story/**` 与必要的共享 ui 组件。
- 沿用 `@dnd-kit/*`、Zustand、`@agentdash/ui`，不引入新的状态库或组件库。
- 性能：Story ≤ 300 时不出现明显卡顿；picker 初次打开 < 100ms。
- 视觉令牌完全沿用现有 oklch token，不新增颜色变量。

## Risks

- **快捷键冲突**：与浏览器/操作系统/编辑器的快捷键冲突需仔细甄别；`Cmd+K` 在 mac Safari 是聚焦地址栏，需要在 capture 阶段拦截或换 `Cmd+P`。
- **Picker 通用化重构**：可能涉及 Field / Select 组件的扩展或 `@agentdash/ui` 新增 `PropertyPicker` 原语；如果通用层改动大，再评估是否拆子任务。
- **批量操作的 race**：连点批改与单条编辑可能撞车，需要 store 的 mutation 原子化（按需顺序串行或乐观回滚）。
