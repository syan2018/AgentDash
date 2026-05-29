# 前端 server-state 与组件结构重构设计

## 目标边界

本轮先把 wave2 复核确认的真实残留落到可验收切片：

1. 让 `features/` / `stores/` 中出现真实 React Query 采用，而不是只 wired `QueryClientProvider`。
2. 先迁移低耦合 server-state：LLM Provider 与 Routine；`projectStore` 仍保留 active project 与 project-agent 等全局事实，后续再分拆。
3. 清掉 active project 双源、store 内跨 store 命令式调用、`workflowStore.selectedActivityKey` 这类一事实双字段。
4. 拆分仍超标的 `SettingsPageContent.tsx` 与 `activity-inspector.tsx`；`components/layout/workspace-layout.tsx` 当前 442 行，不作为超标对象。

## React Query 分层

- `src/api/queryClient.ts` 继续作为全局 client。
- 新增/使用 `src/queries/*` 承载 query key、query hook、mutation hook 与 invalidation。
- HTTP 调用进入 `src/services/*`。LLM Provider 当前已有 `api/llmProviders.ts`，本轮可以先在 query hook 中直接复用其 typed API；Routine 需要新增 `services/routine.ts` 收口路径与 flattened response mapper。
- Zustand 只保留 UI 状态或跨页面本地选择状态。若某 store 仅缓存后端列表和 loading/error，应删除或停止被 UI 消费。

## 第一批数据迁移

### LLM Provider

当前消费点只有：

- `features/settings/ui/SettingsPageContent.tsx`
- `features/settings/ui/UserByokSection.tsx`

迁移为：

- `useLlmProvidersQuery()`：管理员 provider 列表。
- `useCreateLlmProviderMutation()` / `useUpdateLlmProviderMutation()` / `useDeleteLlmProviderMutation()` / `useReorderLlmProvidersMutation()`：成功后 invalidate admin list。
- `useEffectiveLlmProvidersQuery()`：用户 BYOK provider 列表。
- `useSaveUserCredentialMutation()` / `useVerifyUserCredentialMutation()` / `useDeleteUserCredentialMutation()`：成功后 invalidate effective list。

`llmProviderStore.ts` 删除，组件直接消费 hooks 的 `data` / `isPending` / mutation `isPending` / `error`。

### Routine

当前消费点：

- `features/routine/routine-tab-view.tsx`
- `features/routine/routine-dialog-sidebar.tsx`
- `features/routine/execution-history-panel.tsx`

迁移为：

- `services/routine.ts`：list/create/update/delete/enable/regenerateToken/listExecutions。
- `queries/routines.ts`：`useProjectRoutinesQuery(projectId)`、routine mutations、`useRoutineExecutionsQuery(routineId)`。
- UI 局部状态继续放组件内：创建弹窗、编辑 id、删除确认、history panel、token alert。

`routineStore.ts` 删除，列表和执行历史由 query cache 承担。Mutation 成功后 invalidate project routines 或 routine executions。

## Store 耦合收敛

- active project 单源：`projectStore.currentProjectId` 是唯一选择事实。`eventStore` 不再保存 `activeProjectId`；事件流连接元数据若需要去重，只保存连接对象或内部 module 变量，不表达业务 active project。
- project event 分发：`eventStore` 只负责连接、生命周期状态和事件发布；Story/Coordinator 的响应通过显式 subscription 或 App 级 wiring 注册，避免 store 文件之间互相 import 并 `getState()` 调动作。
- `sessionHistoryStore.createNew` 不再读取 `projectStore.getState()`，调用方传入 `projectId`。
- `workflowStore.selectedActivityKey` 删除。Activity key 由 `lifecycleEditor.selection.kind === "activity"` 派生，测试和 UI 改用 selector/helper。

## 组件拆分

- `SettingsPageContent.tsx` 按设置 section 拆到 `features/settings/ui/*Section.tsx` 或已有 section 文件；主文件只保留 tab/section 编排与数据 hook 组合，目标 < 600 行。
- `activity-inspector.tsx` 按 Activity 基础信息、executor、ports、completion、iteration/join、transition inspector 等切出子组件，目标 < 600 行。
- 不改变视觉结构，不新增解释性 UI 文案。

## 验收与缩窄

- `rg "useQuery|useMutation" packages/app-web/src/features packages/app-web/src/stores` 显著大于 0。
- `rg "activeProjectId" packages/app-web/src/stores/eventStore.ts` 无命中。
- `rg "getState\\(\\)\\.(handleStateChange|fetchBackends)" packages/app-web/src/stores` 无命中。
- `rg "selectedActivityKey" packages/app-web/src/stores/workflowStore.ts` 无命中。
- `SettingsPageContent.tsx` 与 `activity-inspector.tsx` 行数 < 600。
- `pnpm -C packages/app-web exec tsc --noEmit` 通过；必要时运行相关 Vitest。

本轮暂不迁 `storyStore`、`workspaceStore`、`projectStore` 全量 server-state。它们消费面大，且包含事件流 patch、active project、project-agent config 等本地导航事实，应在 LLM/Routine 切片稳定后继续拆。
