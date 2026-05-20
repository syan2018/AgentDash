# Design — Story 体验对齐 Multica

PRD: `prd.md`. 本文聚焦技术设计，不重述需求。

## 范围地图（文件 → 改动点）

| 文件 | 改动 |
|------|------|
| `packages/app-web/src/features/story/story-list-view.tsx` | 工具栏抽离；list 行加 checkbox；调用上层 view-state hook |
| `packages/app-web/src/features/story/story-board.tsx` | 消费同一 view-state；列头 inline quick-add；空态 CTA |
| `packages/app-web/src/features/story/story-card.tsx` | 卡片徽章替换为 picker trigger；hover 描述 popover |
| `packages/app-web/src/features/story/story-tab-view.tsx` | 提升 view-state 至此；下挂 toolbar；视图切换保持 state |
| `packages/app-web/src/pages/StoryPage.tsx` | 移除 `StoryStatusActions` 分散按钮；改为 Inspector 顶部 next-step CTA + status picker |
| `packages/app-web/src/features/story/create-task-panel.tsx` | Context 选择增加 select all/none/invert；kind 图标 tooltip |
| `packages/app-web/src/features/story/quick-add.tsx`（新） | board 列内 inline quick-add 输入条 |
| `packages/app-web/src/features/story/story-bulk-toolbar.tsx`（新） | floating batch actions toolbar |
| `packages/app-web/src/features/story/story-quick-jump.tsx`（新） | Cmd+K command palette |
| `packages/app-web/src/features/story/keyboard.ts`（新） | useStoryHotkeys hook（路由作用域） |
| `packages/app-web/src/components/ui/property-picker.tsx`（新） | 通用 PropertyPicker 原语 |
| `packages/app-web/src/components/ui/tooltip.tsx` | 已存在则复用；缺则新增轻量包装 |
| `packages/app-web/src/stores/storyStore.ts` | 新增 `batchUpdateStories(ids, patch)`、`batchDeleteStories(ids)` |
| `packages/app-web/src/stores/storyViewStore.ts`（新） | 视图状态（filter/sort/scope/search/viewMode/selection）独立 store |

## 模块设计

### 1. View State Store（`storyViewStore.ts`）

把 `story-list-view.tsx` 当前内联的 `useState`（filter / sort / scope / search / viewMode）抽成一个 Zustand store，以便 board 与 list 共享、跨视图切换不丢失。

```ts
type StoryViewState = {
  search: string;
  scope: 'all' | 'active' | 'done';
  status: StoryStatus | 'all';
  priority: StoryPriority | 'all';
  type: StoryType | 'all';
  sort: 'priority' | 'updated' | 'title';
  viewMode: 'board' | 'list';
  selectedIds: Set<string>;
  // setters: setSearch / setScope / ... / toggleSelect / clearSelection
};
```

- 不持久化（per-session 即可）；
- `selectedIds` 留在同一个 store，便于 toolbar 跨组件感知；
- 派生过滤逻辑（filter + sort）抽到 `selectFilteredStories(state, stories)` 纯函数，board 和 list 共用，避免双份计算分歧。

### 2. PropertyPicker 原语

抽 `components/ui/property-picker.tsx`，签名借鉴 Multica：

```tsx
type PropertyPickerProps<T> = {
  trigger: React.ReactNode;       // 徽章本体
  value: T;
  options: Array<{ value: T; label: string; icon?: React.ReactNode; tone?: Tone }>;
  onChange: (next: T) => void;
  searchable?: boolean;
  align?: 'start' | 'center' | 'end';
};
```

实现细节：
- 用 `@base-ui/react` 的 Popover；项目里若已有 `Select` 用同一 primitive，否则封 `Popover` + 自管 list；
- 键盘：↑/↓ 切 highlighted（DOM `data-highlighted` 控制样式），Enter 触发选中，Esc 关闭；
- IME 安全：维护 `isComposing` 标志（`compositionstart/end`），composing 中不消化方向键；
- 单结果自动 highlight；
- 受控 open；外部可强制关闭。

### 3. 卡片徽章 → Picker Trigger

`story-card.tsx`：现有 `StoryStatusBadge` / `StoryPriorityBadge` / `StoryTypeBadge` 都是纯展示。改成两层：

- 保留 *Badge 纯展示组件不变；
- 新增 `<EditableStatusBadge story onChange />` 等三个轻包装：内部用 PropertyPicker，trigger = 原 Badge，`stopPropagation` 避免冒泡到卡片导航。

`story-list-view.tsx` 行内的徽章同样替换。

### 4. 状态控件统一

当前 `StoryPage.tsx` 同时有：
- 顶栏的 `StoryStatusActions`（一组语义按钮）
- 右侧 Properties 编辑模式下的 status `Select`

改为：
- 顶栏只保留 **next-step CTA**（按当前 status 推断下一步：draft → "标记就绪"，ready → "开始执行"，running → "提交评审"…）。CTA 调用 `updateStory` 用预计算映射。
- 状态本身的展示 + 直接跳转任意状态：放在 Inspector 顶部用同一个 `EditableStatusBadge`。
- Properties 编辑模式取消 status / priority / type 的 Select；这三个改用 detail 区现有的 EditableBadge 直接 inline 改。Tags 与 description 仍在编辑模式里改（自由文本不适合 picker）。

映射表：

```ts
const NEXT_STEP: Partial<Record<StoryStatus, { to: StoryStatus; label: string }>> = {
  draft: { to: 'ready', label: '标记就绪' },
  ready: { to: 'running', label: '开始执行' },
  running: { to: 'review', label: '提交评审' },
  review: { to: 'completed', label: '标记完成' },
};
```

`failed` / `cancelled` / `completed` 不显示 CTA。

### 5. Quick Add（行内创建）

`quick-add.tsx`：
- 受控展开（点击列头 "+" 后 setOpen(true)）；
- 列顶 ghost 输入框 + 自动聚焦；
- Enter 创建（调用 `createStory({ title, status: column.status, priority: 'P2', type: 'feature' })`）；
- 创建成功后清空 input、保持聚焦（连续输入）；
- Esc 退出 + 收起。
- 失败：行内红字提示，input 保留。

完整表单 drawer 保留作为 "更多字段" 入口（行内右侧加链接 "More fields..." 直接迁移当前 title/draft 进 drawer）。

Drawer 加 `keepOpen` 开关：
- 状态在 `storyViewStore.keepOpenOnCreate` 持久（per-session）；
- 提交成功后：keepOpen=true → 清空表单 + 维持 open；keepOpen=false → 关闭。

### 6. 键盘快捷键（`keyboard.ts`）

实现一个轻量 `useStoryHotkeys`：

```ts
useStoryHotkeys({
  'mod+n': () => openCreate(),
  'mod+k': () => setQuickJumpOpen(true),
  // 卡片聚焦时（document.activeElement 是 [data-story-card-id] 时）：
  'e':     focusedStoryId && (() => navigate(`/story/${focusedStoryId}`)),
  'x':     focusedStoryId && (() => toggleDone(focusedStoryId)),
  'p':     focusedStoryId && (() => openPriorityPickerFor(focusedStoryId)),
});
```

实现要点：
- 在 `StoryPage` / `story-tab-view` 顶层挂 `useEffect` 注册 `window.addEventListener('keydown', handler, true)`，路由切换时清理；
- 输入态判定：`document.activeElement` 是 `INPUT/TEXTAREA/[contenteditable]` 时跳过；
- `mod` = Mac `metaKey` / Win `ctrlKey`；
- `Cmd+K`：在 capture 阶段 preventDefault，避免被浏览器吞；
- 卡片聚焦：每张卡片 `tabIndex={0}`、`data-story-card-id={story.id}`，焦点时显示 outline ring（已有 `focus-visible:ring-2 ring-primary/50` 配色）。

### 7. Quick Jump（`story-quick-jump.tsx`）

简易实现：
- `Cmd+K` 打开一个全屏 backdrop + 居中 input + 结果 list；
- 仅匹配当前 project 的 stories（`storyStore.stories`），title / description / story_key 模糊匹配；
- 上下方向键 + Enter 跳转 `/story/:id`；Esc 关闭。
- 用 `react-virtuoso` 虚拟化（已是依赖）—— 若嫌重也可以前 50 条直接渲染。

### 8. Bulk Toolbar（`story-bulk-toolbar.tsx`）

- `selectedIds.size > 0` 时挂载，固定底部 `fixed bottom-4 left-1/2 -translate-x-1/2 z-50`；
- 内容：count + Status picker + Priority picker + Delete（带确认）+ Clear；
- 调用 store 的 `batchUpdateStories(ids, patch)`：内部循环调用 `updateStory`（如果 API 不支持批量，先串行；后端有批量接口再切）；
- 删除走 `DangerConfirmDialog`，与单条删一致。

### 9. List Row Checkbox

参考 Multica：默认显示 priority 图标，hover 或已选中切换为 checkbox。
- 用 CSS group hover 实现，无 JS：`group-hover/row:hidden` / `group-hover/row:flex`；
- 选中态强制显示 checkbox；
- 表头不需要 "select all"（先做最小版）。

### 10. Context 选择 helper

`create-task-panel.tsx` 的 context list：
- 列表上方加按钮组：`全选 / 反选 / 清空`；用 `Set<contextId>` 管理；
- 项右上：context kind icon + hover tooltip（kind 名称 + 概览）。

### 11. Tooltip & Hover Description

如果 `@agentdash/ui` 没有 Tooltip：
- 临时用 `title` 属性兜底；
- 后续补 `Tooltip` 组件（Base UI 的 Tooltip 已是依赖）。

卡片 hover 描述 popover：
- `onMouseEnter` 延迟 400ms 触发 `setShowFull(true)`；
- 离开 200ms 抖动；
- 仅 board 视图启用（list 视图列宽够，不需要）。

## 数据 / 接口契约

- Story 类型字段、API 客户端方法保持不变；
- `storyStore` 新增方法：

```ts
batchUpdateStories(ids: string[], patch: Partial<Story>): Promise<void>
batchDeleteStories(ids: string[]): Promise<void>
```

- 实现：先串行调用现有 `updateStory` / `deleteStory`，期间收集 errors 统一 toast；不引入新 endpoint。

## 跨层 / Compatibility

- 不改 proto / backend；
- 不改路由；
- 现有 `?open_task_id=` 等 deep link 行为保持；
- 移除 `StoryStatusActions` 时检查是否被 PR 链接 / 文档提及（grep）；如有则导出留个 deprecated re-export 一个版本。

## 测试策略

- **单测**：
  - `selectFilteredStories(state, stories)` 纯函数：覆盖 scope/filter/sort 各组合；
  - `useStoryHotkeys`：jsdom 模拟 keydown，断言回调；输入态屏蔽。
- **组件测**（如已有 vitest + RTL）：
  - PropertyPicker：键盘 nav、IME composing、单结果自动 highlight；
  - QuickAdd：Enter 提交 + Esc 取消 + 错误回退；
  - BulkToolbar：选中数变化时显示/隐藏，批改触发 store。
- **手测脚本**（手测 checklist 写入 `implement.md`）：覆盖 PRD 验收项 8 条 + 浏览器实测。

## Rollout

- 单 PR 推（量适中）；
- 不加 feature flag —— Story 模块只此一家；
- 如果 review 时发现 scope 太大，按 S1→S2→S3→S5→S4→S6→S7→S8 顺序拆分（这个顺序是后续依赖最少）。

## Tradeoffs

- **抽 view-state 到 store** vs **保留 list-view 内联 state**：选前者，因为 board/list 共享是 PRD 必需；代价是多一个 store 文件。
- **PropertyPicker 通用化** vs **三处复制**：通用化值得，三处都需要键盘 + IME，复制成本更高。
- **批量串行调用现有 update** vs **新增后端批量 endpoint**：本轮选前者；前端体验已够，后端工作可后续优化。
- **Quick Jump 内嵌于 Story 路由** vs **全局命令面板**：先 Story 路由内做（scope 小、更可控）；全局后续单独立项。
