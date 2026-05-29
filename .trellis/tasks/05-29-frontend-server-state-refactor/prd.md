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
- [ ] `pnpm -C packages/app-web exec tsc --noEmit` 通过
- [ ] server-state 手写 loading/error/竞态被 react-query 取代
- [ ] `sidebarSessionsStore`/`activeSessionsStore` 合并
- [ ] project/story/workspace API 经 service 层
- [ ] 3 个 god component 拆分，单文件 < ~600 行

## Constraints
- 仅改 `packages/`，不动 `crates/`。
- **不要 git commit**，orchestrator gate 后提交。
- 阶段独立，优先 A、B；A 完成即可单独交付。
- 行为/视觉不回归；UI 辅助文字按"用户是否需要"取舍。
