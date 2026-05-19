# 实施计划

## 推进顺序

按"先抽公共组件 → 再做 Marketplace 收口 → 再回头改各 panel"，每个里程碑跑一次 typecheck 看下游影响范围。

### M1. 共用结构迁移

- [ ] 把 `project-agent-view` 里的 `CardMenu` 抽出到 `packages/app-web/src/features/assets-panel/_shared/CardMenu.tsx`，仅迁纯交互结构；让 `project-agent-view` 改为引用它。
- [ ] `assets-panel/_shared/SourceBadge.tsx`：把 Workflow / MCP 各自的 `SourceBadge` 合并成一个，签名 `{ source: LibraryAssetSource | "builtin_seed" | "user" | ... ; installed?: boolean }`。
  - 如果发现 4 个 panel 用的 source 枚举值不一样，就用对外接口规范化、内部映射，不引入新概念。
- [ ] `_shared/PublishedBadge.tsx`：新组件，输入 `{ version }`，输出 `已发布 v{version}` 小徽章。
- [ ] 验证：`pnpm --filter @agentdash/app-web typecheck`。

> 验收门槛：M1 完成后 4 个 panel 视觉应完全等价于原状态（只是引用换了）；提交前手动开 dev 检查。

### M2. PublishLibraryAssetDialog 冲突探测 + 复位

- [ ] 把文件移动到 `assets-panel/publish/PublishLibraryAssetDialog.tsx`，更新 4 处 import。
- [ ] 新增 props：`currentUserId: string | null`（对话框内部不直接读 store，便于复用与测试）。
- [ ] 打开时 `fetchLibraryAssets({ asset_type })` → filter user_authored + 同 key + same owner → 切 `mode: "create" | "update"`。
- [ ] update 模式：预填上次值，version 默认 `suggestNextVersion`，overwrite 默认 true，不再隐藏 overwrite 复选框。
- [ ] 探测网络失败：保留 create 流程，控制台 warn，不阻断；保留服务端 409 兜底。
- [ ] 单测：`PublishLibraryAssetDialog.conflict.test.tsx` 覆盖三分支。
- [ ] 验证：typecheck + 该测试通过。

### M3. Marketplace segmented + "我发布的" 视图

- [ ] 在 `MarketplaceCategoryPanel` header 区加 `viewMode: "all" | "published"` segmented，受 URL `?view=published` 控制。
- [ ] published 视图：在 `fetchLibraryAssets` 调用上叠加 `owner_id = currentUser.user_id`（`ListLibraryAssetsQuery.owner_id` 已支持），并 filter `source === "user_authored"`；空态文案带"发布资产"按钮。
- [ ] published 卡片右上角不再展示"安装/已安装"主按钮，主操作改为"重新发布"，复用 PublishLibraryAssetDialog 的 update 形态（带 `project_asset_id` = null 是不支持的，必须沿着已存在的 user_authored 资产由后端带回）。
  - 风险点：现有 publish API 入参依赖 `project_asset_id`，"重新发布"没有原 project asset 选择步骤会卡。
  - 兜底：published 卡片"重新发布"实际打开 `AssetPickerDrawer`，type 锁死、key 预填，让用户重新选项目资产，等同于"先选源 → 发布并覆盖"。
- [ ] 用户未登录时 segmented disabled + tip。
- [ ] 单测：`Marketplace.published-view.test.tsx` 覆盖 filter 正确。
- [ ] 验证：typecheck + 测试 + dev 视觉。

### M4. AssetPickerDrawer

- [ ] 新建 `assets-panel/publish/AssetPickerDrawer.tsx`：drawer 形态（与 `MarketplaceAssetDrawer` 视觉对齐），两步——选 type → 选具体 project asset。
- [ ] 4 类资产的 list 拉取：
  - Agent：从 `useProjectStore` / 现有 agent 服务拿 link 列表。
  - MCP：复用 `services/mcpPresets.listMcpPresets`。
  - Workflow：复用 `services/workflows.fetchLifecycles`。
  - Skill：复用 `services/skills.listProjectSkills`。
- [ ] 选中后挂 `PublishLibraryAssetDialog`。发布完成后关 drawer，触发 marketplace reload，并把 segmented 切到 `published`。
- [ ] 单测：覆盖 type 切换、list 渲染、选中触发 onPick。
- [ ] 验证：typecheck + 测试 + dev 实际跑通 4 类发布闭环。

### M5. 项目资产页快捷入口收敛

- [ ] **WorkflowCategoryPanel**：`LifecycleAssetCard` footer 改为"编辑/查看"主按钮 + CardMenu。CardMenu 项：发布到资源市场（仅 source !== builtin_seed && !installed_source 时显示）、删除。
  - 卡片右上角加 `<PublishedBadge />`（在 user_authored 同 key 已发布时显示）。
- [ ] **SkillCategoryPanel**：`SkillGrid` footer 同步改造。
- [ ] **McpPresetCategoryPanel**：`McpPresetCard` footer 把"编辑/查看"主按钮 + "复制为 user"作为单独按钮（保留显眼度）+ CardMenu(发布、删除)。
- [ ] **project-agent-view**：保留现状，但确认 installed agent 的 CardMenu 不再显示发布项；与 unified CardMenu 接口对齐。
- [ ] 验证：typecheck + 4 类测试 + dev 视觉。

### M6. 文案与 hint

- [ ] `AssetsTabView` 里 `资源市场` 的 hint 改成 `从公共库浏览、安装与发布资产`。
- [ ] AssetPickerDrawer / PublishedAssetsView 文案统一为"发布到资源市场"/"我的发布"。
- [ ] 验证：dev 视觉过一遍。

### M7. 收尾

- [ ] `pnpm --filter @agentdash/app-web typecheck` 全绿。
- [ ] `pnpm --filter @agentdash/app-web test` 现有 + 新增测试通过。
- [ ] `pnpm dev` 起 web → 走过下面的 happy paths：
  - 新建一个 user_authored Agent → Marketplace 发布资产 → 选 Agent → 选目标 → 发布成功 → 在"我发布的"里看到。
  - 在项目资产页 Workflow 卡片菜单内"发布到资源市场" → 同上闭环。
  - 已发布过的资产：项目资产卡片显示"已发布 v1.0.0"；再次走发布流程 → dialog 默认进入 update 形态。
  - 已 installed 的资产：CardMenu 不显示发布项。
- [ ] 等用户视觉验收（per memory：未明确批准前不要 commit）。
- [ ] commit + spec 更新（如需）。

## 风险与回滚点

- M1 抽 CardMenu / SourceBadge 会触达 4+ 个文件，回滚点：单 commit，便于 revert 单步。
- M3 published 视图依赖 `LibraryAssetDto.owner_id` + `CurrentUser.user_id`（已确认存在，`ListLibraryAssetsQuery.owner_id` 已支持），无需改后端契约。
- M4 4 类 list 服务是否已经是 pure async function；若部分逻辑深埋 store，先做最小 service 抽出。

## 关键校验命令

```bash
pnpm --filter @agentdash/app-web typecheck
pnpm --filter @agentdash/app-web test
pnpm --filter @agentdash/app-web dev
```

## Follow-ups（不属于本任务，记录待开新任务）

- ConfirmDeleteDialog / ConfirmOverwriteDialog 跨面板归一。
- "我发布的" 视图添加废弃 / 取消发布动作。
- Marketplace 卡片"已安装"跳回项目资产对应位置。
- Skill OriginBadge 与 SourceBadge 概念重叠的彻底归并。
