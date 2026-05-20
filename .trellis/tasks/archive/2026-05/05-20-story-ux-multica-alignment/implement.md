# Implement — Story 体验对齐 Multica

PRD: `prd.md`. Design: `design.md`. 本文是有序执行清单 + 验证命令 + 回滚锚点。

按以下顺序推进；每个里程碑结束都跑 [Validation](#validation) 一轮。

## M0 — 基线锁定

- [ ] 当前 main HEAD 为 `f16d60a8`；确认 `git status` clean。
- [ ] 全跑一遍现有 typecheck / lint / 单测，记录基线。
  - `pnpm -F app-web typecheck`
  - `pnpm -F app-web lint`
  - `pnpm -F app-web test --run`（若有）
- [ ] 浏览器中过一遍 Story 现状（创建 → 看板 → 详情 → 任务），截图存 `research/baseline/`，便于事后对比。

**回滚锚点：** M0 结束打 tag `pre-story-ux-rebase`（可选）。

## M1 — View State Store + 共享 toolbar（落 S1）

1. [ ] 新建 `packages/app-web/src/stores/storyViewStore.ts`，结构见 design §1。
2. [ ] 抽 `selectFilteredStories(state, stories)` 纯函数到 `features/story/select-stories.ts`，附单测。
3. [ ] `story-list-view.tsx`：删除局部 `useState` 的 search/scope/filter/sort/viewMode；改读 store。
4. [ ] `story-tab-view.tsx`（或新建 `story-toolbar.tsx`）：把当前 toolbar JSX 抽出来，board 与 list 视图前都先渲染 toolbar。
5. [ ] `story-board.tsx`：消费 store 的 filtered + sorted 列表（之前是接 props 的全量 stories）。
6. [ ] 视图切换不丢 state：手测 list 中输入 search → 切到 board → 仍生效。

**Validation：**
```bash
pnpm -F app-web typecheck
pnpm -F app-web lint
pnpm -F app-web test --run -- select-stories
```

## M2 — PropertyPicker 原语（为 S2/S3 铺路）

1. [ ] 新建 `packages/app-web/src/components/ui/property-picker.tsx`，签名见 design §2。
2. [ ] 实现键盘 ↑/↓/Enter/Esc + IME 安全（compositionstart/end）。
3. [ ] 单测：键盘 nav、IME composing、Esc 关闭。
4. [ ] 在 `components/ui/index.ts` 导出。

**Validation：**
```bash
pnpm -F app-web typecheck
pnpm -F app-web test --run -- property-picker
```

## M3 — 行内徽章 picker（落 S2 + S3）

1. [ ] 新建 `EditableStatusBadge` / `EditablePriorityBadge` / `EditableTypeBadge`（同文件或邻近）。每个内部用 PropertyPicker，trigger = 现有展示 Badge。
2. [ ] `story-card.tsx`：把展示 badge 替换为 editable 版本；点击徽章 `e.stopPropagation()` 防卡片导航。
3. [ ] `story-list-view.tsx` 行内同步替换。
4. [ ] `StoryPage.tsx`：
   - 移除 `StoryStatusActions`（保留文件、注释 deprecated 一个版本，或直接删—— grep 确认无外部引用后删）；
   - Inspector 顶部加 next-step CTA（按 design §4 映射表）；
   - Properties 编辑模式去掉 status/priority/type 的 Select；status 行直接放 EditableStatusBadge（同样三个）。
5. [ ] 手测：板 / 列 / 详情三处改 status，互相联动且 store 单一更新路径。

**Validation：** typecheck + lint + 手测；确认 Properties edit 模式只剩 tags + description。

## M4 — Quick Add 行内创建（落 S4）

1. [ ] 新建 `features/story/quick-add.tsx`：受控 open，input + 提示符 + 错误显示。
2. [ ] `story-board.tsx` 列头 "+" 改为 toggle quick-add（替代直接打开 drawer）；列头新增 "More fields..." 链接打开 drawer。
3. [ ] CreateStoryDrawer：加 `Keep open` 开关；提交成功 + keepOpen 时清空 + 保留焦点；状态从打开时上下文预填。
4. [ ] 手测：列 "+" → 输入标题 → Enter → 立即出现卡片，input 仍聚焦能继续输入；Esc 收起。

**Validation：** typecheck + lint + 手测。

## M5 — 键盘快捷键（落 S5）

1. [ ] 新建 `features/story/keyboard.ts` 与 `useStoryHotkeys` hook，按 design §6 实现。
2. [ ] `story-tab-view.tsx` 顶层挂 hook，绑定 `mod+n` / `mod+k`。
3. [ ] `story-card.tsx`：`tabIndex={0}` + `data-story-card-id`；focused state 视觉。
4. [ ] 卡片 focused 时 `e/x/p` 生效。
5. [ ] 手测：在 input 聚焦时按 `n` 不应触发；Esc 不要破坏现有 picker 关闭逻辑。

**Validation：** typecheck + lint + 浏览器手测全套快捷键。

## M6 — Quick Jump（落 S5 一部分）

1. [ ] 新建 `features/story/story-quick-jump.tsx`：portal、search input、虚拟列表（前 50 条直渲染先）。
2. [ ] `Cmd+K` 触发开启；选中后 `navigate(/story/:id)`。
3. [ ] 手测：浏览器/IDE 的 `Cmd+K` 是否被吞；如被吞，capture 阶段 preventDefault。

**Validation：** typecheck + lint + 手测。

## M7 — 多选 + 批量 toolbar（落 S6）

1. [ ] `storyStore.ts` 加 `batchUpdateStories` / `batchDeleteStories`（串行实现）。
2. [ ] `story-list-view.tsx` 行内：默认 priority 图标；hover/选中显示 checkbox（CSS group hover）。
3. [ ] 新建 `story-bulk-toolbar.tsx`，挂在 `story-tab-view` 内，`selectedIds.size > 0` 时显示。
4. [ ] toolbar 里复用 PropertyPicker 改 status/priority；删除走 `DangerConfirmDialog`。
5. [ ] 操作完成后 `clearSelection`。
6. [ ] 手测：选 3 条 → 批改 status → 看板/列表都刷新；批删 → 计数归零、toolbar 收起。

**Validation：** typecheck + lint + 手测。

## M8 — Context 选择 helper + Tooltip + 微交互（落 S7 + S8）

1. [ ] `create-task-panel.tsx`：context list 上方加 `全选 / 反选 / 清空`。
2. [ ] kind icon hover tooltip（如 `@agentdash/ui` 无 Tooltip 则用 `title` 兜底）。
3. [ ] 徽章 P0/P1/P2/P3、FEAT/BUG... hover tooltip（同上）。
4. [ ] 卡片 hover 描述 popover（仅 board 视图，400ms 延迟）。
5. [ ] 空列 "Create in this column" 文案核对统一。
6. [ ] 手测覆盖 8 条 PRD 验收项。

**Validation：**
```bash
pnpm -F app-web typecheck
pnpm -F app-web lint
pnpm -F app-web test --run
```

## M9 — 收尾

1. [ ] 跑 `python ./.trellis/scripts/task.py validate`（如适用）。
2. [ ] 走 trellis-check 流程：spec 合规、跨层数据流、code reuse、一致性。
3. [ ] 自检 `Out of Scope` 清单，确认没误伤。
4. [ ] 更新 `.trellis/spec/frontend/` 内可能影响的设计 token 文档（仅当真有改动时）。
5. [ ] 等用户在浏览器中视觉验收通过 → 提交（`feedback_no_commit_until_approved`）。

## Validation

每个里程碑都跑：

```bash
pnpm -F app-web typecheck
pnpm -F app-web lint
pnpm -F app-web test --run    # 若 vitest 配置存在
```

最后一轮额外：

```bash
pnpm -F app-web build         # 验证 production 构建不破
```

手测 golden path（M9 终验）：
1. 进入某 project 的 Story tab，确认 toolbar 在 board / list 都显示。
2. 板视图列头 "+" → inline 输入 → 创建；连续创建 3 条。
3. 拖动卡片改 status；点击卡片徽章弹 picker 改 priority；键盘选项。
4. 进 detail 页，next-step CTA 行得通；Inspector 内 status/priority/type 用 picker。
5. 列表视图 hover 行 → 显 checkbox；多选 3 条 → toolbar 出现 → 批改状态。
6. `Cmd+N` 建 story；`Cmd+K` 跳转任意 story；卡片聚焦后 `E/X/P`。
7. 在 detail 内创建 task，context 区试用全选/反选/清空。
8. 视觉巡检：徽章 tooltip / hover 描述 popover / 空列 CTA。

## Rollback Points

- M1 结束未达预期 → 回滚 store + 共享 toolbar 抽离，回到 list-view 局部 state。
- M3 结束 picker 体验差 → 暂时把 detail 改回 Select 即可，卡片层 picker 留着。
- M7 批量出现并发 bug → 先把 toolbar 隐藏（`selectedIds` 强制 clear），保留 store 方法。

## Review Gates

- M3 结束（picker 全员就位）后请用户在浏览器中实操一次，确认 picker 触发位置、样式、键盘体验对路再继续。
- M5 + M6 结束（键盘 + quick jump）后再确认快捷键无冲突。
- M9 终验由用户视觉验收后再提交。
