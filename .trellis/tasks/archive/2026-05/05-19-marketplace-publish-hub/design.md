# Marketplace 收口发布流程 — 技术设计

## 1. 边界与数据流

### 1.1 涉及文件总览（按修改预期分组）

| 角色 | 路径 | 改动性质 |
| --- | --- | --- |
| Marketplace 主体 | `packages/app-web/src/features/assets-panel/categories/MarketplaceCategoryPanel.tsx` | 大改：增 segmented + 发布主入口 + 资产选择器接入 |
| 发布 dialog | `packages/app-web/src/features/assets-panel/categories/PublishLibraryAssetDialog.tsx` | 中改：冲突探测、形态切换 |
| Workflow 面板 | `packages/app-web/src/features/assets-panel/categories/WorkflowCategoryPanel.tsx` | 改：移除 footer 发布按钮，改 CardMenu；引入 publish-from-card 适配 |
| Skill 面板 | `packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx` | 同上 |
| MCP 面板 | `packages/app-web/src/features/assets-panel/categories/McpPresetCategoryPanel.tsx` | 同上（保留 "复制为 user" 不动） |
| Agent 视图 | `packages/app-web/src/features/project/project-agent-view.tsx` | 小改：CardMenu 项与新统一 menu primitive 对齐；过滤掉 installed agent 的发布项 |
| 资产选择器 | 新增：`packages/app-web/src/features/assets-panel/publish/AssetPickerDrawer.tsx` | 新文件 |
| Publish hub 子视图 | 新增：`packages/app-web/src/features/assets-panel/publish/PublishedAssetsView.tsx` | 新文件，"我发布的" segmented 渲染 |
| 共享组件归位 | 候选：`packages/ui/src/primitives/SourceBadge.tsx` 或保留 `_shared/SourceBadge.tsx` | 视实施过程判断 |

### 1.2 顶层数据流

```
MarketplaceCategoryPanel
├── segmented: 浏览全部 / 我发布的            (本地 state)
├── view = 浏览全部
│   └── 既有逻辑：fetchLibraryAssets + sourceStatus + grid + drawer
├── view = 我发布的
│   └── fetchLibraryAssets({ owner_id: currentUser.user_id }) +
│       client-side filter source === "user_authored"
└── 主操作区: 发布资产 → AssetPickerDrawer
                       └── 选 type → 选 project asset → PublishLibraryAssetDialog
                                                          └── onPublished:
                                                                - reload Marketplace
                                                                - segmented 切到"我发布的"
```

`AssetPickerDrawer` 内部按 type 调用现有服务：

| Asset type | List 数据源 | Defaults |
| --- | --- | --- |
| Agent (`project_agent`) | `useProjectStore` 当前 project 的 agents/links（与 project-agent-view 一致的 source） | preset_name / display_name / description |
| MCP (`mcp_preset`) | `services/mcpPresets`（与 McpPresetCategoryPanel 一致） | key / display_name / description |
| Workflow (`workflow_bundle`) | `services/workflows` 的 lifecycles | key / name / description |
| Skill (`skill_asset`) | `services/skills` 的 listProjectSkills | key / display_name / description |

> 关键约束：`AssetPickerDrawer` 直接复用各 panel 已有的 list 入口，不引入新的 fetcher。如果某个 panel 把 list 逻辑深埋在组件内部 hook 中，先做小重构把 fetcher 抽到 `services/*` 层（pure function），不引入 store 复用。

### 1.3 Publish dialog 冲突探测

打开 `PublishLibraryAssetDialog` 时立即触发：

```ts
fetchLibraryAssets({
  asset_type: kindToAssetType(props.assetKind),
}).then((list) => {
  const existing = list.find(
    (a) => a.key === defaults.key && a.source === "user_authored"
            && a.owner_id === currentUserId,
  );
  if (existing) {
    setMode("update");
    setVersion(suggestNextVersion(existing.version));
    setDisplayName(existing.display_name);
    setDescription(existing.description ?? "");
    setOverwrite(true);
  }
});
```

`suggestNextVersion("1.0.0")` 用最简语义：解析 `MAJOR.MINOR.PATCH`，patch+1；不规范字符串 fallback `${input}+1`。冲突探测失败（网络错误）只 log warning，不阻塞用户继续走原流程，落到 409 兜底。

## 2. CardMenu 统一

### 2.1 现状

Agent 视图已有 `CardMenu` 组件 ([project-agent-view.tsx:673](../../../packages/app-web/src/features/project/project-agent-view.tsx#L673))，签名：
```ts
items: { key: string; label: string; danger?: boolean; onSelect: () => void }[]
```

Workflow / Skill / MCP 没有 CardMenu，footer 是行内多个 button。

### 2.2 设计

把 Agent 视图里的 `CardMenu` 抽到 `packages/app-web/src/features/assets-panel/_shared/CardMenu.tsx`（不下沉到 ui，因为它依赖 Portal/dropdown 行为，且是 panel-specific 的视觉语言）。

> 实施时如果发现现有 CardMenu 实现在 project-agent-view 内部耦合 ProjectAgent 数据，则只搬"纯交互结构"（trigger + dropdown + items），不带数据绑定。原 Agent 视图改为消费搬出去的版本。

新搬的 CardMenu 在 Workflow/Skill/MCP 卡片 footer 替换原"编辑/发布/删除"行内按钮组：保留主操作"编辑"为主按钮，其他动作（包括"发布到资源市场"、"删除"）进 menu。

## 3. 已发布徽章

`packages/app-web/src/features/assets-panel/_shared/PublishedBadge.tsx` 读取一个简单的 `published` map：在每个 panel 加载完 list 时，按 `(asset_kind, key)` 在已加载的 LibraryAssets 里查找 user_authored 同 key 项 → 输出 `{ version }`。

实现路径：每个 panel 已经会调用 `fetchLibraryAssets`（Marketplace 在用），但分类面板自己没有；增量代价是各 panel 多发一次 list 请求 + filter，可接受。如果实施时发现可以从 `installed_source` 反推则更省，本期以"显式 fetch + filter"为准。

## 4. 共用组件迁移决策表

| 组件 | 当前位置 | 复用情况 | 决策 |
| --- | --- | --- | --- |
| PublishLibraryAssetDialog | `assets-panel/categories/` | 4 处使用 | 移到 `assets-panel/publish/` 子目录（仍在 app-web） |
| SourceBadge | Workflow/MCP 各自定义 | 重复实现 | 提取到 `assets-panel/_shared/SourceBadge.tsx`；如确认零业务依赖再搬 `packages/ui` |
| ConfirmDeleteDialog / ConfirmOverwriteDialog | 各 panel | 多次重写 | 本次不动（out-of-scope），但记录到 implement.md 末尾 follow-ups |
| CardMenu | project-agent-view 内部 | 即将多处使用 | 抽到 `assets-panel/_shared/CardMenu.tsx` |
| Notice (`_shared/Notice`) | 已经在 `_shared` | 多处使用 | 维持现状 |

> 决策原则：本期只做"先识别 → 再决定"，不为未来预留接口；运行过程中发现还有 2 处以上重复的展示组件可上报但不强制本期处理。

## 5. 路由 / 导航

侧栏 [`AssetsTabView.tsx`](../../../packages/app-web/src/features/assets-panel/AssetsTabView.tsx) 里"资源市场"的 hint 文案改为 `从公共库浏览、安装与发布资产`，其它结构不动。Marketplace 内部新增 segmented 不引入新路由（用 URL search param `?view=published` 反向同步，便于卡片菜单"已发布"徽章未来跳转，但本期默认不点击不跳转）。

## 6. 兼容性 / 风险

- **冲突探测多发一个请求**：Marketplace 已经在 list；项目资产页 panel 加载也会触发一次 fetchLibraryAssets。可接受；后续如果性能成问题可以加 store cache，本期不上。
- **移除 footer 发布按钮的发现性**：通过 CardMenu 让"发布"对所有非 installed 资产都可见，不会丢失能力。Marketplace 主入口是更显眼的发现路径。
- **Marketplace "我发布的" 当前用户判定**：依赖 `useCurrentUserStore().currentUser.user_id`；若 currentUser 为 null（未登录或 fetch 失败），segmented 按钮 disabled + tip。
- **AssetPickerDrawer 体积**：4 类资产的 list 入口规整后预计 < 250 行；如果某类的 list 逻辑搬移代价过大，可在 implement.md 里降级为"先打开对应分类面板再用 CardMenu 发布"，保留快捷入口路径。

## 7. 回滚

- 新增文件可直接删除恢复。
- Marketplace、PublishDialog、3 个 panel 的改动通过单 commit 落，便于 revert。
- 不涉及数据库 / API / 持久化，回滚成本=代码 revert。

## 8. 测试策略

- AssetPickerDrawer：单测覆盖 type 选择 → list 渲染 → 选中后回调（用 mock service）。
- PublishLibraryAssetDialog 冲突探测：单测覆盖三分支（无冲突 / 有冲突 / 探测失败 fallback）。
- Marketplace segmented：单测 `view=published` 时 filter 正确。
- 现有 e2e（Playwright）若覆盖到旧 footer "发布"按钮则需要同步更新选择器。
