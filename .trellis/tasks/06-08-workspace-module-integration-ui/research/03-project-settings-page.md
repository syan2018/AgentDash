# Research: 项目设置页位置与 section 结构（挂 WorkspaceModule 区块）

- **Query**: 项目级设置页位置 / ProjectSettings / Extension 管理 UI 现状 / 现有 section 结构
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `packages/app-web/src/pages/ProjectSettingsPage.tsx` | 项目级设置页主组件（1379 行） |
| `packages/app-web/src/App.tsx` | 路由挂载（`/projects/:projectId/settings`） |
| `packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx` | 现成的项目级 Canvas 管理 UI（消费 extension-runtime） |
| `packages/app-web/src/features/extension-runtime/ui/ExtensionWebviewPanel.tsx` / `ExtensionCanvasPanel.tsx` | session 内的 extension 渲染面板 |

### ProjectSettingsPage 的 Tab / Section 结构

`SettingsTab` 类型（行 34）四个 tab，`SETTINGS_TABS`（行 42-47）：
- `overview` 概览（基础信息 / 访问摘要 / 调度安全网）
- `context` VFS 资源（`ContextTabContent`，行 319：Project VFS Mount / 解析后 Mount / Runtime Preview）
- `workspace` 工作空间（`BackendAccessPanel` 行 365 + `WorkspaceList` 行 1098）
- `management` 管理动作（共享管理 / 模板与复制 / 危险操作）

渲染入口在 `ProjectSettingsPage`（行 634），tab 切换 `activeTab === "..."` 条件渲染（行 992-1351）。

### 复用组件（页面内私有）

- `SectionCard({title, description, children})`（行 99）：每个大区块外框（`<section>` + h2 + 描述）。
- `ContentGroup({title, description, children})`（行 119）：区块内分组（带上分隔线，h3 大写标题）。
- `TabButton`（行 139）：tab 切换按钮。

新增 WorkspaceModule 管理区块的两种落点：
1. **新增 tab**（如 `modules` / `workspace-modules`）：在 `SettingsTab` 联合类型（行 34）加值、`SETTINGS_TABS`（行 42）加项、底部加 `activeTab === "modules" && (...)` 分支。
2. **挂进现有 tab**：`workspace` tab 已聚合 backend/workspace；或 `context` tab。用 `<SectionCard>` + `<ContentGroup>` 复用样式。

### Extension 管理 UI 现状

- **本页面没有"项目已装插件列表"区块**——ProjectSettingsPage 不消费 extension-runtime（grep `useProjectExtensionRuntime` 命中列表里无此文件）。
- 现成 extension/canvas 管理消费点（grep `useProjectExtensionRuntime`/`installations`）：
  - `features/canvas-panel/ProjectCanvasManager.tsx`（项目级 Canvas 管理）
  - `features/assets-panel/categories/MarketplaceCategoryPanel.tsx` / `MarketplaceAssetDrawer.tsx` / `CanvasCategoryPanel.tsx`（资产面板里的安装/管理）
  - `features/extension-runtime/ui/*`、`features/workspace-panel/WorkspacePanel.tsx`、`pages/SessionPage.tsx`（session 内渲染）
- 即"项目已装插件 + 可见 canvas 的合并管理列表"目前**不存在**，正是本 child R1 要新建的区块。

### 状态/权限上下文（页面已有）

- `project.access.can_edit` / `can_manage_sharing`（行 773-774）：编辑权限门控（可见性裁切的启停按钮应据此 disable）。
- `useProjectStore` / `useWorkspaceStore` / `useCoordinatorStore`（行 638-653）：现成 store 注入范式。

## Caveats / Not Found

- 路由路径需在 `App.tsx` 确认（grep 命中 `ProjectSettings`）；本调研未展开 App.tsx 具体 route 行号，但页面通过 `useParams<{projectId}>`（行 636）取 id，路径形如 `/projects/:projectId/settings`（见 `handleCloneProject` 行 906 `navigate(/projects/${cloned.id}/settings)`）。
- 可见性裁切的"编辑→生效"链路（R2）落在 AgentFrame capability 通道，不是项目 config——UI 写入目标需 design 明确（见 05 文档：当前无 frame 级编辑入口/写路由）。
