# 前端 server-state 与组件结构重构执行计划

## Step 1 · 基线计数

1. 记录 `useQuery|useMutation` 在 `features` / `stores` 的命中数。
2. 记录 store `isLoading|loading|saving|error` 命中数。
3. 记录 `SettingsPageContent.tsx`、`activity-inspector.tsx`、`workspace-layout.tsx` 行数。

## Step 2 · LLM Provider 迁 React Query

1. 新增 `queries/llmProviders.ts`。
2. 将 `SettingsPageContent.tsx` 与 `UserByokSection.tsx` 改用 query/mutation hooks。
3. 删除 `stores/llmProviderStore.ts` 与相关 imports。
4. 运行 grep：`rg "useLlmProviderStore|useLlmByokStore" packages/app-web/src` 无命中。

## Step 3 · Routine 迁 React Query

1. 新增 `services/routine.ts` 收口 API 与 `RoutineCreationResponse -> Routine` mapper。
2. 新增 `queries/routines.ts`。
3. 改 `routine-tab-view.tsx`、`routine-dialog-sidebar.tsx`、`execution-history-panel.tsx` 消费 query/mutation hooks。
4. 删除 `stores/routineStore.ts`。
5. 运行 grep：`rg "useRoutineStore" packages/app-web/src` 无命中。

## Step 4 · Store 双源与命令式耦合

1. `eventStore.ts` 删除 `activeProjectId` 字段，保留连接生命周期。
2. 用显式事件订阅/发布替代 `eventStore` 内直接调用 `useStoryStore.getState().handleStateChange` 与 `useCoordinatorStore.getState().fetchBackends`。
3. `sessionHistoryStore.createNew` 改为由调用方传入 `projectId`。
4. `workflowStore.ts` 删除 `selectedActivityKey` 字段，补 selector/helper 并更新测试。

## Step 5 · God component 拆分

1. 拆 `features/settings/ui/SettingsPageContent.tsx`，主文件 < 600 行。
2. 拆 `features/workflow/ui/activity-inspector.tsx`，主文件 < 600 行。
3. `components/layout/workspace-layout.tsx` 当前 < 600，仅保持不回归。

## Step 6 · 验证与收尾

1. `rg "useQuery|useMutation" packages/app-web/src/features packages/app-web/src/stores`
2. `rg "activeProjectId" packages/app-web/src/stores/eventStore.ts`
3. `rg "getState\\(\\)\\.(handleStateChange|fetchBackends)" packages/app-web/src/stores`
4. `rg "selectedActivityKey" packages/app-web/src/stores/workflowStore.ts`
5. `pnpm -C packages/app-web exec tsc --noEmit`
6. 相关测试：
   - `pnpm -C packages/app-web exec vitest run src/stores/workflowStore.test.ts`
   - 若拆分 settings/workflow UI 触及测试，再跑对应 test。
7. 更新 `progress-checklist.md` 与 task PRD 的 evidence。
