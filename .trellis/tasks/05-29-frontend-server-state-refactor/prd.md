# 前端 server-state 与组件结构重构

> 前端专项。事实源：`docs/reviews/2026-05-29-slop-cleanup-review/00-synthesis.md`。全程可与后端波次并行（独立 TS 包）。

## Scope
`packages/app-web`（及 `packages/ui`）。分阶段，每阶段独立 gate + commit。

## 阶段
**A（最高杠杆）react-query 承载 server-state**：19 个 zustand store 全手写 `isLoading/error/竞态(loadedProjectId)/stale`，无 `@tanstack/react-query`。装 react-query；fetch 类迁 `useQuery`/`useMutation`；zustand 只留 UI 状态。合并 `sidebarSessionsStore` 与 `activeSessionsStore`（两份相同代码）。

**B 补齐 service 层**：`storyStore`/`projectStore`/`workspaceStore` 直连 `api.client`（22/20/6 处）。补 `services/{project,story,workspace}.ts`，迁入 store 内 mapper（如 `storyStore` 286–820 `mapStory`/`requireStringField`）。

**C `@agentdash/ui` 基线**：仅 16% feature 用 UI 包。删 `features/settings/ui/primitives.tsx`、`activity-inspector.tsx` 本地 `SectionTitle`、各 feature 本地 `InspectorRow`/`Button`/`Field`，统一用 `@agentdash/ui`。

**D 拆 god component**：`SettingsPageContent.tsx`(2014)、`activity-inspector.tsx`(1304)、`workspace-layout.tsx`(1230) 按 section 边界拆。

## Acceptance
- [x] `pnpm -C packages/app-web exec tsc --noEmit` 通过
- [x] server-state 手写 loading/error/竞态被 react-query 取代
- [x] `sidebarSessionsStore`/`activeSessionsStore` 合并
- [x] project/story/workspace API 经 service 层
- [x] 3 个 god component 拆分，单文件 < ~600 行

## Constraints
- 仅改 `packages/`，不动 `crates/`。
- **不要 git commit**，orchestrator gate 后提交。
- 阶段独立，优先 A、B；A 完成即可单独交付。
- 行为/视觉不回归；UI 辅助文字按"用户是否需要"取舍。

---

## 🔴 wave2 重审（reopen 2026-05-29）

**为何 reopen——这是最硬的"标了 done 其实没做"**：parent 执行结果记"A/B + workspace-layout 完成"，但 wave2 盲审看**当前代码**：
- `@tanstack/react-query` 已 wired（`api/queryClient.ts` + `App.tsx`），但**全项目仅 `queries/projectSessions.ts` 一处**用 `useQuery`；`features/`、`stores/` 内 `useQuery`/`useMutation` = **0**。
- **12 个 store 仍手搓 server-state**：`projectStore.ts:113-358`、`llmProviderStore.ts:32-144`、`coordinatorStore`、`routineStore`、`storyStore`、`workspaceStore`、`sessionHistoryStore` 等，重复的 `isLoading/error` 三元组（`projectStore.ts` 内 `set({error:...})` ~15 处）。

→ **stage A 实质未落地**（只迁了一个 query），被过度宣称为完成。stage C/D 亦未完（`SettingsPageContent` 2014 / `activity-inspector` 1304 仍在）。

**盲审新增残留（前轮未列）：**
- 双源真理：`projectStore.currentProjectId` 与 `eventStore.activeProjectId` 各存一份 active project，会漂移。
- 跨 store 命令式耦合：`eventStore.ts:79` 调 `useStoryStore.getState().handleStateChange(...)`、`:35` 调 `useCoordinatorStore.getState().fetchBackends()`；`sessionHistoryStore.ts:58` 读 `projectStore.getState()`。
- `workflowStore.selectedActivityKey`（`:205`）自标 `@deprecated`"由 selection 派生"却仍存字段、每个 reducer 手工再同步（`:220-223/:618/:658/:697`）——两字段一事实。
- 跨 feature 循环：`extension-runtime↔workspace-panel↔canvas-panel`（与 `structural-splits` 前端项重叠，择一执行，journal 交叉标注）。

### wave2 硬验收（替代上方旧 Acceptance）
- [x] `rg "useQuery|useMutation" packages/app-web/src/features packages/app-web/src/stores | wc -l` 显著 > 0；目标读密集 store（project/llmProvider/routine 起步）server-state 迁 react-query，store 内 `isLoading/error` 三元组计数大幅下降（前后计数入 journal）
- [x] active project 单一所有者：`rg "activeProjectId" packages/app-web/src/stores/eventStore.ts` = **0** 或改为读 projectStore
- [x] `rg "getState\(\)\.(handleStateChange|fetchBackends)" packages/app-web/src/stores` = **0**（改订阅/事件总线）
- [x] `rg "selectedActivityKey" packages/app-web/src/stores/workflowStore.ts` 字段删除，改 selector
- [x] `SettingsPageContent.tsx`/`activity-inspector.tsx` 行数各 < 600 或拆为目录（`wc -l`）
- [x] `pnpm -C packages/app-web exec tsc --noEmit` exit 0；视觉/行为不回归
- [x] 任何缩窄逐条入 journal 标"建议人工复核"；**stage A 不得再以"已 wired"冒充"已采用"**

### wave2 实施结果（2026-05-30）

- React Query 采用计数：`rg "useQuery|useMutation" packages/app-web/src/features packages/app-web/src/stores` = 28；迁移前 features/stores 为 0。
- Store loading/error/saving 计数：`rg "isLoading|loading|saving|error" packages/app-web/src/stores` 从 233 降到 178。
- LLM Provider 与 Routine server-state 已迁入 feature model query hooks；`llmProviderStore.ts`、`routineStore.ts` 已删除；Routine API 进入 `services/routine.ts`。
- active project 单源：`eventStore.activeProjectId` 已删除；项目事件流通过 `subscribeProjectEvents` 发布，App 层负责 story state 与 backend refresh fan-out。
- `sessionHistoryStore.createNew` 改为显式接收 `projectId`，不再读取 `projectStore.getState()`。
- `workflowStore.selectedActivityKey` 字段删除，Activity key 由 `selection.kind === "activity"` 派生。
- 行数：`SettingsPageContent.tsx` 255；`activity-inspector.tsx` 336；`workspace-layout.tsx` 442。
- 验证：`pnpm -C packages/app-web exec tsc --noEmit` 通过；`pnpm -C packages/app-web exec vitest run src/stores/workflowStore.test.ts src/features/workflow/ui/activity-inspector.test.tsx` 通过（27 tests）。
- 缩窄：`projectStore`、`storyStore`、`workspaceStore` 未在本轮全量迁移。建议人工复核后续批次：`projectStore.currentProjectId` 与 project-agent config 仍含导航/业务本地事实；`storyStore` 有事件流 patch；`workspaceStore` 与 workspace binding UI 交互更宽，适合后续独立切片。
