# 资源市场前端整体交互优化 — Implement

## 顺序与门禁

按下面顺序逐项实现，每个 step 完成后跑 `pnpm --filter app-web typecheck`，
全部完成后再统一 build + 视觉验收。

### Step 1 · payload 形状取样（≈ 5min）

- [ ] 启 dev server，调一次 `GET /shared-library/assets?include_deprecated=true`，把
      返回 4 类资产的 payload 各取一例存到工作区 `_payload-samples.json`（不进 git）。
- [ ] 在 `.trellis/tasks/05-18-marketplace-ux-overhaul/research/` 下记录 4 类 payload
      实际字段名映射，作为 drawer 解析依据。

> **门禁**：不取样直接写 drawer 等于拍脑袋。如果 dev 起不来，回退到从
> `crates/agentdash-application/src/shared_library/install.rs` 反推，但要把推断点写进
> research 注明"待运行验证"。

### Step 2 · 抽出 `<Notice>` 共享组件（≈ 15min）

- [ ] 新建 `packages/app-web/src/features/assets-panel/_shared/Notice.tsx`，签名按
      design.md。
- [ ] 在 `index.ts` 导出（如该文件已存在）。
- [ ] `MarketplaceCategoryPanel.tsx` 引用 `<Notice>` 替换 `error`/`message` 两块；
      合并 state 为单一 `notice`。
- [ ] `SkillCategoryPanel.tsx` 替换内部 `function Notice(...)` 为 import 共享版。
- [ ] `WorkflowCategoryPanel.tsx` 替换 inline 成功 / 错误条。
- [ ] `McpPresetCategoryPanel.tsx` 替换 inline 成功 / 错误条。
- [ ] `pnpm --filter app-web typecheck` 通过。

> **回滚点**：若 typecheck 失败超过 10 分钟未定位，单独 revert Step 2 的引用替换，保
> 留 `<Notice>` 文件本身。Marketplace 后续步骤不依赖它。

### Step 3 · `installSummaryByAssetId` 派生 + 卡片状态合并（≈ 25min）

- [ ] `MarketplaceCategoryPanel.tsx` 新增 `useMemo` 派生 `installSummaryByAssetId`。
- [ ] `LibraryAssetCard` 改名为 `MarketplaceAssetCard`，prop 从 `sourceStatus` 改为
      `installSummary`；右上角同时渲染 `type chip` + 新的
      `<InstallStatusChip summary={summary} />`。
- [ ] `<InstallStatusChip>` hover tooltip 列出 `installations`：`mcp_preset · my-fs`
      这种格式，每行一条。Tooltip 用原生 `title` 即可（避免引入 popover 依赖）。
- [ ] **删除底部 "项目安装来源" section** 整段。

### Step 4 · 类型筛选 segmented + 搜索框（≈ 20min）

- [ ] 顶部 `<select>` 替换为按钮组：复用 `agentdash-button-secondary` 风格，active
      态加 `border-primary/40 bg-secondary/70`。5 个按钮：全部 / Agent / MCP /
      Workflow / Skill。
- [ ] 新增 `<SearchInput>`：单 `<input className="agentdash-form-input">`，受控绑定
      `searchTerm`，placeholder `按名称 / 描述 / key 搜索`。
- [ ] 派生 `visibleAssets`，grid 改用它。
- [ ] 切换类型保留 `searchTerm`（不要 reset）。

### Step 5 · 详情抽屉（≈ 60min，最大块）

- [ ] 新建 `MarketplaceAssetDrawer.tsx`，含 `<MarketplaceAssetDrawer>` +
      `<ConfirmOverwriteDialog>` + 4 个 `*Body` + `<RawPayloadFallback>`。
- [ ] 卡片 footer 加 `[详情]` 按钮，点击 setDrawer({open, assetId})。
- [ ] 抽屉内 footer 安装按钮逻辑：
      - 无 `installSummary` → 直接 `onInstall(false)`。
      - `update_available` → setOverwrite({open, ...})，等 confirm 再调 `onInstall(true)`。
      - `up_to_date` → disabled，文案"已是最新"。
      - `source_missing` → disabled，文案"来源缺失"，带 tooltip 解释。
- [ ] 安装/更新成功后：关闭抽屉 + 关闭 confirm + 触发 `load()` 刷新。

### Step 6 · 覆写确认接入卡片（≈ 10min）

- [ ] 卡片上的"安装到项目 / 更新到项目"按钮也走同一 `tryInstall(asset)` 帮手：
      - 内部判断是否需要 confirm。
      - 抽屉里复用同一 helper。

### Step 7a · 侧栏归位 Marketplace 入口（≈ 20min）

- [ ] 在 `AssetsTabView.tsx` 把现有 `CATEGORIES` 拆成 `PRIMARY_CATEGORIES`（移除
      marketplace 项）和 `SOURCE_CATEGORIES`（仅含 marketplace）。
- [ ] aside 内部布局改为 `flex h-full flex-col`：主类目 nav 在上，divider + 副标题
      `安装入口` + `<NavItem variant="source" />` 推到底（`mt-auto`）。
- [ ] 抽出 `<NavItem>` 组件，按 variant 切两套样式（参考 design.md 表）。
- [ ] download icon：inline svg，16px，line variant，描边色 `currentColor`，避免
      主题切换时颜色硬编码。
- [ ] 路由 `/dashboard/assets/marketplace` 不动，验证从主类目切到 Marketplace +
      返回时 active 状态正确。

### Step 7b · seed 按钮收进 empty-state（≈ 10min）

- [ ] 顶部移除"同步内置资源"按钮。
- [ ] `assets.length === 0 && !loading` 的 empty cell 内渲染：
      - 文案：`当前类目暂无资源`。
      - 副文案：`可点击下方按钮加载内置示例（不影响项目数据）`。
      - 按钮：`加载内置示例` → 调 `seedBuiltinLibraryAssets({asset_type})`。
- [ ] 列表非空时按钮永不出现。
- [ ] 仍保留 `busyAssetId === "__seed__"` 的 busy 状态，但仅作用于 empty-state 按钮。

### Step 8 · 验收（≈ 20min）

- [ ] `pnpm --filter app-web typecheck` 通过。
- [ ] `pnpm --filter app-web build` 通过。
- [ ] dev server 启动，手测 7 个 AC：
      1. 资产卡片右上角 chip 状态正确，hover 显示安装拓扑。
      2. 4 类资产打开详情抽屉，分别看到 type-specific body；payload 异常时落 fallback。
      3. 类型按钮组切换 + 搜索框过滤双向工作。
      4. 三个邻居 panel 引用共享 Notice，4 秒消失，× 关闭可点。
      5. 已装 + update_available 资产，点更新先弹 confirm；首装无 confirm。
      6. 列表为空时 empty-state 出现 seed 按钮；列表非空 header 不再有 seed 按钮。
      7. 左侧侧栏：主类目 4 项在上，分隔线 + "安装入口" 小标题 + Marketplace 入口
         贴底；active/hover 样式与主类目区分明显；窄高度时不重叠。
- [ ] 截图前后对比；等用户视觉验收（参考 [feedback_no_commit_until_approved]，
      typecheck 通过不代表可以 commit）。

## 验证命令

```bash
pnpm --filter app-web typecheck
pnpm --filter app-web build
pnpm --filter app-web dev   # 手动 UI 验收
```

## 回滚点

| Step | 回滚动作 |
|------|---------|
| Step 2 | revert 各 panel 引用，保留 `_shared/Notice.tsx` 文件以便后续重用 |
| Step 3 | revert MarketplaceCategoryPanel 单文件即可恢复底部 panel |
| Step 5 | drawer 文件未被卡片引用前可独立存在；revert 卡片改动即可隐藏入口 |
| Step 7a | revert AssetsTabView.tsx 单文件，恢复 5 项平级 NavLink |
| Step 7b | revert empty-state 改动 + 把 header seed 按钮 cherry-pick 回来 |

## Out of Scope（避免 scope creep）

- ❌ 给 `LibraryAssetDto.payload` 写 TS 类型（留下一个任务）。
- ❌ 抽屉的"上一条/下一条"导航。
- ❌ 安装后跳转到对应类目页面 + 高亮新装资产（仅 toast，不做 navigate）。
- ❌ 给 mcp transport 在 drawer 里跑 probe（用 payload 静态字段）。
- ❌ 把 Skill/Workflow/MCP 类目的来源 chip 视觉重做。
