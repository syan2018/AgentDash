# Marketplace 收口发布流程并统一资产卡片交互

## Goal

把"用户发布资产到 Shared Library"的主路径从 4 个项目资产分类面板收口到 Marketplace 内部，让 Marketplace 同时承担"浏览/安装/发布/管理我的发布"四个角色；项目资产页只保留快捷入口，不再承担发布主流程。同时清理沿途暴露出来的交互不一致，并把多处复用的展示组件抽到合适的 package。

## Background

- 05-18-marketplace-user-publish-assets 任务在 PRD 第 59 条把发布入口写到了"Project Assets 卡片或详情中"，并把 Marketplace 是否需要"我发布的"视图列为 Open Question，结果上线后这个 Open Question 没收口。
- 现状：[PublishLibraryAssetDialog](../../../packages/app-web/src/features/assets-panel/categories/PublishLibraryAssetDialog.tsx) 被 [project-agent-view](../../../packages/app-web/src/features/project/project-agent-view.tsx)（CardMenu）、[WorkflowCategoryPanel](../../../packages/app-web/src/features/assets-panel/categories/WorkflowCategoryPanel.tsx)（footer 内联按钮）、[SkillCategoryPanel](../../../packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx)（footer 内联按钮）、[McpPresetCategoryPanel](../../../packages/app-web/src/features/assets-panel/categories/McpPresetCategoryPanel.tsx)（footer 内联按钮）4 处各自挂载，行为相似但形态不一致。
- [MarketplaceCategoryPanel](../../../packages/app-web/src/features/assets-panel/categories/MarketplaceCategoryPanel.tsx) 只承担"浏览 + 安装"，[AssetsTabView](../../../packages/app-web/src/features/assets-panel/AssetsTabView.tsx) 在侧栏把"资源市场"分组到"安装入口"，从命名到行为都把 Marketplace 钉死在消费者角色。
- `LibraryAssetDto.source` 已经支持 `user_authored`，后端 publish API（`POST /shared-library/publish`）也是按 `(asset_kind, project_asset_id)` 入参，不依赖前端切入位置，因此发布主路径的搬家不需要改后端契约。

## Foundational Principles

- Marketplace 是发布、浏览、安装、管理我发布资产的**唯一主入口**；项目资产页提供的是"反向快捷"，不重复承担引导职责。
- 项目资产卡片应该把"发布到资源市场"这种创建新实体到另一个系统的动作和"编辑/删除当前资产"区分开——这些动作必须从 footer 主区移走，统一塞进 CardMenu/`...`，避免主次不分。
- 发布对话框应该在用户提交前就告诉他冲突，不让"必先吃 409 才能勾覆盖"变成隐形步骤。
- 多处重复的展示组件先识别再判断要不要拆包；本次只迁移有强复用证据的组件，避免顺手抽象。

## Requirements

### A. Marketplace 收口发布主路径

1. Marketplace 顶部新增 segmented 控件：`浏览全部 / 我发布的`，默认"浏览全部"。"我发布的"仅展示当前用户作为 owner 的 `source = user_authored` 资产，并在卡片上能直观看到对应的版本与摘要。
2. Marketplace 顶部主操作区新增"发布资产"按钮，点击后打开**资产选择器**（drawer 或 modal）：
   - 第一步选 asset type（Agent / MCP / Workflow / Skill）。
   - 第二步基于当前 project 列出可发布的项目资产（复用各 panel 里现有的 list 服务，不重复实现拉取逻辑）。
   - 选定后进入复用的 `PublishLibraryAssetDialog`，发布完成回到 Marketplace 并刷新当前视图（如在"我发布的"分组下应能看到新资产）。
3. 在"我发布的"视图里，资产卡片新增"重新发布（更新版本）"快捷动作，复用同一 dialog；其他卡片动作（删除/废弃）本期 out-of-scope，但不能再展示与发布场景冲突的"安装/已安装"主按钮。

### B. 项目资产页清理为快捷入口

4. Workflow / Skill / MCP Preset 的卡片 footer 移除独立"发布"按钮，统一改为：每张卡片右上角加 CardMenu/`...` 入口（如已有则复用），把"发布到资源市场"项放到菜单内。
5. Agent 已经走 CardMenu，本期保持现有交互即可，但要：
   - 与新引入的统一 CardMenu 组件对齐样式与 onSelect 签名。
6. 已经从市场安装回来的资产（`builtin_seed`、有 `installed_source` 的项），不应展示"发布"项，避免循环发布。
7. 项目资产卡片在 source/installed 状态外，新增轻量"已发布 vX.Y.Z"指示（数据可由 list 接口的 source/version 推导，不引入额外 API）。

### C. 发布对话框冲突体验

8. `PublishLibraryAssetDialog` 在打开时根据 `(asset_kind, key)` 主动调用 list（前端已有 `fetchLibraryAssets` 服务），探测是否存在同 key 已发布资产：
   - 若不存在：保持现有"发布新版本"流程。
   - 若存在：对话框默认进入"更新发布"形态，预填上次的 display_name/description，version 输入框预置一个建议 patch 增量（例如 `1.0.0` → `1.0.1`），并默认勾选 overwrite，不再依赖 409 触发 UI 切换。
9. 仍需保留服务端 409 兜底处理，但前端 happy path 不再依赖它。

### D. 共用组件归位（运行中识别 + 迁移）

10. 实施过程中识别出"明显跨面板/跨页面复用"的展示组件，按以下优先级搬家：
    - 纯展示、零业务依赖：迁到 `packages/ui/src/primitives`（例：SourceBadge）。
    - 依赖 `services/*`、`stores/*` 等 app-web 特化层：保留在 `packages/app-web/src/features/assets-panel/_shared`，不下沉到 ui。
11. 搬家必须 1:1 行为对齐，不顺手扩展接口；只允许补足"被多处需要的最小可选 prop"。

## Out of Scope

- "我发布的"视图的删除/取消发布/废弃管理（独立任务）。
- Marketplace 卡片"已安装"跳转回项目资产位置。
- AssetsTabView 侧栏分组语义重构（只改 hint 文本，不动结构）。
- 后端 API/契约修改：包括 publish endpoint 入参、SourceStatus DTO 等。
- 本次涉及的资产类型扩展（Extension 等）。
- "复制为 user"概念是否扩展到 Workflow/Skill/Agent。

## Acceptance Criteria

- [ ] Marketplace 顶部存在 `浏览全部 / 我发布的` segmented 控件，"我发布的"只列出当前用户 user_authored 资产，且空态有清晰文案与"发布资产"快捷入口。
- [ ] Marketplace 主操作区"发布资产"按钮打开资产选择器，能完成 4 类资产（Agent/MCP/Workflow/Skill）的发布闭环，发布成功后"我发布的"视图能立即看到新资产。
- [ ] 4 类项目资产卡片 footer 不再出现独立"发布"按钮；发布动作统一收敛到卡片菜单 / 详情菜单内。
- [ ] 已从市场安装的资产（含 builtin / 含 installed_source）卡片菜单不展示"发布"项。
- [ ] 项目资产卡片对已发布资产展示"已发布 vX"指示。
- [ ] PublishLibraryAssetDialog 打开时若同 key 已存在则进入"更新发布"形态，且不需要先吃 409。
- [ ] 至少 1 个以上明显复用的展示组件按规则迁移到 ui 或 _shared 并被两处以上引用。
- [ ] 前端 `pnpm --filter @agentdash/app-web typecheck` 通过；现有相关测试通过；新增的资产选择器关键行为或 publish dialog 冲突分支有测试覆盖。
- [ ] 用户视觉验收（dev 启动 → 项目资产/Marketplace 双向通路体验一致）。

## Open Questions

- 资产选择器的 UI 形态优先建议 drawer（与 [MarketplaceAssetDrawer](../../../packages/app-web/src/features/assets-panel/categories/MarketplaceAssetDrawer.tsx) 对齐），如设计偏好不同请在审 PRD 时指出。
- "已发布 vX" 指示是否需要点击直接跳到 Marketplace 的"我发布的"对应卡片？本期默认不做跳转（仅静态徽章）。
