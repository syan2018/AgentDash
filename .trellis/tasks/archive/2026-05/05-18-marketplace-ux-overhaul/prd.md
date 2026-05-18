# 资源市场前端整体交互优化 — PRD

## 背景

`MarketplaceCategoryPanel.tsx`（311 行）是公共资源市场前端的唯一入口。当前实现把后端
`ProjectAssetSourceStatusDto` 的 5 个数组原样平铺渲染，导致信息架构、卡片信息密度、
反馈语言、覆写语义等多处和邻居 `SkillCategoryPanel` / `WorkflowCategoryPanel` /
`McpPresetCategoryPanel` 不一致，用户主观感受"诡异"。

本任务做一轮整体打磨，目标是把"资源市场"做成一个内敛、可读、可控的安装入口，让
"看 → 选 → 装 → 用"的链路顺滑。

## 范围

**纳入范围**（本轮交付）：

1. 信息架构：移除底部"项目安装来源"独立区块，状态合并到卡片右上角 chip。
2. 卡片详情：每张卡片新增"详情"按钮，弹右侧抽屉，按 `asset_type` 自适应展示 manifest 关键字段。
3. 类型筛选：原生 `<select>` 换成 segmented control / button group，全部 + 4 类。
4. 名称搜索：加单输入框，前端过滤 `display_name` / `description` / `key`。
5. 反馈一致性：抽出共享 `<Notice>`（4s auto-dismiss + 关闭按钮），与邻居 panel 对齐。
6. 覆写确认：点"更新到项目"先弹 confirm modal，提示"将覆盖本地修改"。
7. seed 入口收敛：`同步内置资源`按钮从常驻 header 收进 empty-state 引导位。
8. 侧栏归位：在 `AssetsTabView` 类目栏中把 Marketplace 从平级 NavLink 挪到栏底固定区，加分隔线 + 专属"安装入口"样式。

**不纳入范围**（留给后续任务）：

- 后端 manifest payload schema 变更或新增字段。
- Skill/Workflow/MCP 各自类目页的进一步改造（本轮只改 chip 颜色对齐，不改结构）。
- 离线/远端 Skill 导入流程的 UX。
- 安装后的"在 X 类目中查看"跳转动画/高亮（仅做最小 navigate）。

## 用户故事

**US-1**: 作为项目维护者，我想一眼看到某条市场资产是否已安装到本项目、是否有新版，
不再需要在卡片和底部列表之间来回比对。

**US-2**: 作为想试用一个 Workflow 模板的用户，我希望先看到它有几个 step、targets 是
谁、描述写了啥，再决定是否安装。Skill / MCP / Agent Template 同理。

**US-3**: 作为已安装但本地改过 Skill 的用户，当我点"更新到项目"时，我希望系统先提
醒我"会覆盖本地修改"，而不是悄悄盖掉。

**US-4**: 作为新进项目的用户，我希望进入资源市场后，知道这个页面是干嘛的；如果是空
的，引导我先种内置资源；不要把"同步内置资源"这种工程内部按钮常驻在顶部。

**US-5**: 作为资产数量较多时使用市场的用户，我希望能输关键词缩小范围，按类型 tab 切
换分类。

**US-6**: 作为日常浏览项目资产的用户，我希望左侧类目栏先映入眼帘的是项目里"已经有
什么"（Workflow/Canvas/MCP/Skill），而"去哪儿装新的"作为辅助入口固定在栏底，不抢
主类目的视觉权重。

## 非目标用户故事

- 暂不要求"对比 v1/v2 diff"——本轮覆写仅做文案警告。
- 暂不要求安装多 kind 的同一资产联动展示完整安装拓扑——chip tooltip 列出 1 行/kind 即可。
- 暂不重做 Skill/Workflow/MCP 类目的整体布局。

## 验收标准

**AC-1（信息架构）**：
- `MarketplaceCategoryPanel` 不再渲染 "项目安装来源" 独立 section。
- 每张 `LibraryAssetCard` 右上角同时显示 type chip 和 install-status chip：
  - 未安装：仅 type chip。
  - 已安装/有更新/来源缺失：status chip 文案对应 `已安装` / `有 v{x}` / `来源缺失`。
- 同一 `library_asset_id` 装到多个 kind 时，status chip 取"最坏状态"（priority:
  source_missing > update_available > up_to_date），hover tooltip 列出每个 kind+key。

**AC-2（详情抽屉）**：
- 每张卡片新增 `详情` 按钮，点击后从右侧滑入抽屉（不覆盖卡片网格的滚动状态）。
- 抽屉头：`display_name` + `version` + type chip + status chip + `安装到项目` 按钮。
- 抽屉体按 `asset_type` 自适应：
  - `skill_template`：description + 文件列表（path + bytes）+ SKILL.md frontmatter 摘要。
  - `workflow_template`：description + step 表（name/key/agent）+ edge 数 + target_kinds。
  - `mcp_server_template`：description + transport 块（type/url/command）+ 静态 tools 列表（来自 payload，不主动 probe，避免抽屉慢）。
  - `agent_template`：description + 默认 preset 摘要（model/system prompt 摘要 + tool count）。
- payload 字段缺失时，回退展示 `description + raw JSON 折叠区`，不报错。
- ESC / 点遮罩 / 关闭按钮均可关闭。

**AC-3（类型筛选 + 搜索）**：
- 顶部把原生 `<select>` 换成 segmented button group：`全部 / Agent / MCP / Workflow / Skill`。
- 顶部加单行 `<input>` 搜索框，placeholder `按名称 / 描述 / key 搜索`。
- 类型筛选请求后端，搜索仅前端过滤；二者可叠加。
- 切换类型保留搜索词。

**AC-4（反馈一致性）**：
- 抽出 `<Notice>` 到 `features/assets-panel/_shared/Notice.tsx`，3 个邻居 panel 同步引用（替换各自重复实现）。
- 4 秒 auto-dismiss + 右上角 `×` 关闭。
- 错误（destructive 配色）+ 成功（emerald 配色）两个 tone。

**AC-5（覆写确认）**：
- 当 `sourceStatus === "update_available"` 时，点击"更新到项目"先弹 `<ConfirmOverwriteDialog>`：
  - 文案：`将更新「{display_name}」 v{installedVersion} → v{libraryVersion}。本地若有未同步修改将被覆盖。`
  - 按钮：`取消` / `覆盖更新`。
- 首次安装（无 `installed_source`）保持原直接安装，不弹确认。

**AC-6（seed 收敛）**：
- 顶部移除常驻"同步内置资源"按钮。
- 列表为空时，empty-state 内容包含一个 `加载内置示例` 按钮，触发 `seedBuiltinLibraryAssets`。
- 列表非空时，永远不显示该按钮。

**AC-7（不破坏现有契约）**：
- 后端 API 调用方法、参数、响应解析全部保持不变。
- `installLibraryAsset` 签名不动；`overwrite` 行为不动（仍由前端控制）。
- Skill / Workflow / MCP 类目页面除引用 `<Notice>` 外，不引入其他视觉行为变更。

**AC-8（侧栏归位）**：
- `AssetsTabView` 的 `CATEGORIES` 数组只保留 `workflow / canvas / mcp-preset / skill`
  四个项目资产类目；Marketplace 不再混在主 nav 里。
- 主 nav 下方加 `border-t border-border` 分隔线 + `STATIC SOURCES` / `安装入口` 小
  标题（与主类目的 `类目` 标题视觉对齐但语义区分）。
- 分隔线下渲染 Marketplace 专属 NavLink：
  - 文案：`资源市场`，hint：`从公共库安装资产`。
  - 视觉：和主类目相同 active/idle 状态机，但 idle 态用更弱的边框/底色（如
    `bg-secondary/20`），active 时变成 `bg-primary/8` + 主色文字，区别于主类目的
    `bg-secondary/70`。
  - 左侧加一个 16px icon（download / sparkle 都行，inline svg，不引依赖）。
- 路由保持 `/dashboard/assets/marketplace`，URL/路由配置无需改动。
- 当 Marketplace 选中时，aside 区不抢标题语义——header 的"项目资产"文案保持，
  Marketplace 自己的 panel header 已有 `Shared Library / 资源市场` 二级标题。

## 约束

- **只动 frontend**（`packages/app-web`）。后端契约不变。
- **不引入新依赖**。抽屉 / dialog 沿用现有 `fixed inset-0 + onClick stopPropagation` 模式。
- **保持 Tailwind class token 一致**：复用 `agentdash-button-primary` / `agentdash-button-secondary` / `agentdash-form-input` 等已有 class。
- **手动 typecheck + 跑过 web 端**：跑 `pnpm --filter app-web typecheck` 和最少 `pnpm --filter app-web build`。可视交互验证由用户视觉验收（参考 [feedback_no_commit_until_approved]）。

## 性能 / 体积

- 详情抽屉 lazy 不强制：组件 inline 即可，资产模板 manifest 体积小（< 50KB）。
- 搜索前端过滤即可，不引入 fuse.js 之类。
- 全部改动控制在 `MarketplaceCategoryPanel.tsx` + 新增 `_shared/Notice.tsx` + 新增 `MarketplaceAssetDrawer.tsx`，避免扩散到非市场代码。
