# 资源市场前端整体交互优化 — Design

## 边界

**范围内文件**：

- `packages/app-web/src/features/assets-panel/categories/MarketplaceCategoryPanel.tsx` — 主重构目标
- `packages/app-web/src/features/assets-panel/AssetsTabView.tsx` — 侧栏拆分主类目 / Marketplace 入口区
- `packages/app-web/src/features/assets-panel/_shared/Notice.tsx` — 新增（共享反馈条）
- `packages/app-web/src/features/assets-panel/categories/MarketplaceAssetDrawer.tsx` — 新增（详情抽屉）
- `packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx` — 替换 inline `Notice`
- `packages/app-web/src/features/assets-panel/categories/WorkflowCategoryPanel.tsx` — 替换 inline 反馈条
- `packages/app-web/src/features/assets-panel/categories/McpPresetCategoryPanel.tsx` — 替换 inline 反馈条
- `packages/app-web/src/features/assets-panel/index.ts` — 视情况导出 `Notice`

**范围外**：所有 `services/`, `types/`, `stores/`, 后端 crate。

## 状态模型

### 主面板 state

```ts
type DetailDrawerState =
  | { kind: "closed" }
  | { kind: "open"; assetId: string };

type ConfirmOverwriteState =
  | { kind: "closed" }
  | { kind: "open"; asset: LibraryAssetDto; installedVersion: string };

interface PanelState {
  assetType: LibraryAssetType | "all";
  searchTerm: string;                      // 新增
  assets: LibraryAssetDto[];
  sourceStatus: ProjectAssetSourceStatusDto | null;
  loading: boolean;
  busyAssetId: string | null;
  notice: { tone: "success" | "danger"; message: string } | null;
  drawer: DetailDrawerState;               // 新增
  overwrite: ConfirmOverwriteState;        // 新增
}
```

`message` 与 `error` 合并为单一 `notice`，由 `<Notice>` 渲染。

### 派生数据

```ts
// 一条资产对应的所有安装实例（可能跨 kind）
interface InstallSummary {
  status: SharedLibrarySourceStatus;       // 取最坏
  installations: Array<{
    asset_kind: string;
    project_asset_key: string;
    installed_version: string;
    current_source_version: string | null;
    item_status: SharedLibrarySourceStatus;
  }>;
}

const installSummaryByAssetId: Map<string, InstallSummary>
```

构造方式：把 `sourceStatus` 5 个数组 flatten，按 `library_asset_id` group，
`status` 用 `sourceStatusPriority` 取最坏（沿用现有函数）。

### 视图过滤

```ts
const visibleAssets = useMemo(() => {
  const term = searchTerm.trim().toLowerCase();
  if (!term) return assets;
  return assets.filter(a =>
    a.display_name.toLowerCase().includes(term) ||
    (a.description ?? "").toLowerCase().includes(term) ||
    a.key.toLowerCase().includes(term),
  );
}, [assets, searchTerm]);
```

类型筛选继续走后端（保留 `assetType` query param），便于未来分页。

## 组件契约

### `<Notice>` (新)

```ts
interface NoticeProps {
  notice: { tone: "success" | "danger"; message: string } | null;
  onDismiss: () => void;
  /** 4000ms 默认；0 表示不自动消失 */
  autoHideMs?: number;
}
```

- Mount 时若 `notice && autoHideMs > 0`，启动 setTimeout 调 `onDismiss`。
- `notice` 变化时清旧 timer 启新（依赖 `notice?.message + tone`）。
- `null` 时返回 `null`（不占空间）。

### `<MarketplaceAssetDrawer>` (新)

```ts
interface MarketplaceAssetDrawerProps {
  asset: LibraryAssetDto | null;       // null 时不渲染
  installSummary?: InstallSummary;
  busy: boolean;
  onClose: () => void;
  onInstall: (overwrite: boolean) => void;
}
```

布局：

```
┌─ overlay (fixed inset-0 bg-foreground/18) ─────────┐
│         ┌── drawer (right-0 w-[480px] h-full) ──┐  │
│         │ Header: type chip + display_name     │  │
│         │         status chip + version        │  │
│         │ ────────────────────────────────────  │  │
│         │ Description                          │  │
│         │ ────────────────────────────────────  │  │
│         │ TypeSpecificBody (switch on asset_type)│ │
│         │ ────────────────────────────────────  │  │
│         │ Footer: [关闭] [安装到项目 / 更新] │  │
│         └──────────────────────────────────────┘  │
└────────────────────────────────────────────────────┘
```

`TypeSpecificBody` 通过 `switch (asset.asset_type)` 分发到 4 个内部子组件：

```ts
function SkillTemplateBody({ payload }: { payload: unknown })
function WorkflowTemplateBody({ payload }: { payload: unknown })
function McpServerTemplateBody({ payload }: { payload: unknown })
function AgentTemplateBody({ payload }: { payload: unknown })
```

每个子组件内部用一个轻量 type guard 解 payload；解析失败则 fallback 到
`<RawPayloadFallback payload={payload} />`（折叠 JSON 显示）。

#### Payload 形状假设（基于现有 `seed.rs` 和 install 实现，待 implement 阶段对齐）

- skill: `{ asset: SkillAssetSnapshot, files: [{ relative_path, byte_size }] }`
- workflow: `{ lifecycle: LifecycleSnapshot, steps: [...], edges: [...] }`
- mcp_server: `{ preset: McpPresetSnapshot }`
- agent: `{ template: AgentTemplateSnapshot, default_preset: ... }`

> **D2a 风险**：现有前端没有 payload 的 TS 类型声明（`payload: unknown`）。Drawer 必
> 须做防御式解析；未知形状走 fallback。Implement 阶段第一步是开 `LibraryAssetDto.payload`
> 的实际样本（直接调一次接口存 fixture）确认字段名，避免拍脑袋。

### `<ConfirmOverwriteDialog>` (drawer 文件内)

```ts
interface ConfirmOverwriteDialogProps {
  asset: LibraryAssetDto;
  installedVersion: string;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}
```

风格沿用 `McpPresetCategoryPanel.tsx` 内的 `ConfirmDeleteDialog`（`w-[380px]` modal +
backdrop `bg-black/40`）。

### `<MarketplaceAssetCard>` (重构)

新增 prop：

```ts
interface MarketplaceAssetCardProps {
  asset: LibraryAssetDto;
  installSummary?: InstallSummary;       // 替代原 sourceStatus 单值
  busy: boolean;
  onOpenDetail: () => void;              // 新增
  onInstall: (overwrite: boolean) => void;
}
```

底部 actions 从单 `安装` 按钮变为 `[详情] [安装/更新/已安装]` 两键行。

## 数据流

```
load()
 ├─ fetchLibraryAssets({asset_type})       (server filter)
 └─ fetchProjectAssetSourceStatus()
       └─ flatten 5 arrays → installSummaryByAssetId

render
 ├─ <Toolbar> (typeFilter + searchInput + refresh)
 ├─ <Notice notice={notice} ... />
 ├─ visibleAssets = filter(assets, searchTerm)
 │  └─ <Grid>
 │     └─ <MarketplaceAssetCard installSummary=installSummaryByAssetId.get(asset.id) />
 ├─ empty grid → <EmptyState> (含 seed 按钮)
 ├─ drawer.kind === "open" → <MarketplaceAssetDrawer asset={find} />
 └─ overwrite.kind === "open" → <ConfirmOverwriteDialog>

click "安装到项目"  (无 installSummary) → install(asset, false)
click "更新到项目"  (有 update_available) → 先 setOverwrite({open, ...})
ConfirmOverwriteDialog → install(asset, true)
click "详情"        → setDrawer({open, assetId})
drawer 内点 "安装/更新" → 同上
```

## 兼容性

- 三个邻居 panel 替换为共享 `<Notice>` 时，**保留各自的 `setMessage/setError` API**，
  仅把 inline JSX 替换为 `<Notice notice={...} onDismiss={...} />`。
- 任何对 `installLibraryAsset` 的请求体保持不变。
- `seedBuiltinLibraryAssets` 调用入口从 header 移到 empty-state，但请求形状不动。
- 已有 e2e/单测：搜索"MarketplaceCategoryPanel"在仓库中无测试断言（验证步骤：
  implement 阶段 grep 一次确认）。

## 回滚

单 commit + 单文件粒度，回滚走 `git revert`。共享 `<Notice>` 的引入若让邻居 panel 出
问题，单独 revert 引用点，不影响主市场页改造。

## 侧栏分层

### 现状

`AssetsTabView` 的 `CATEGORIES` 数组将 `workflow / marketplace / canvas / mcp-preset
/ skill` 5 个 NavLink 平铺渲染。Marketplace 与其他四者**语义不对等**——其他是"项目
里已有的资产类目"，Marketplace 是"从外部安装的入口"。

### 目标结构

```
aside (w-56)
├── label: 类目
├── nav (flex-col gap-1)
│   ├── NavLink workflow
│   ├── NavLink canvas
│   ├── NavLink mcp-preset
│   └── NavLink skill
├── divider (border-t)
├── label: 安装入口
└── nav (mt-auto 推到底)
    └── NavLink marketplace (专属样式)
```

`mt-auto` 让 Marketplace 区在 aside 高度足够时贴底，aside 短时正常顺序排列（避免重
叠）。aside 整体保持 `flex flex-col`。

### Marketplace NavLink 专属样式

| 状态 | 主类目 NavLink | Marketplace NavLink |
|---|---|---|
| 容器 | `border rounded-[10px] px-3 py-2.5` | 同上 + `flex items-center gap-2` |
| Idle | `text-muted-foreground border-transparent hover:bg-secondary/40` | `text-muted-foreground border-border bg-secondary/20 hover:bg-secondary/40` |
| Active | `border-primary/20 bg-secondary/70 font-medium text-foreground` | `border-primary/30 bg-primary/8 font-medium text-foreground` |
| 左侧 icon | 无 | 16px inline svg（download glyph） |
| 副文案 hint | 有 | 有，但字号同主类目 `text-[11px]` |

差异点（让用户一眼看出"这是入口、不是类目"）：
- idle 态有可见的边框和淡底色（vs 主类目 idle 透明）。
- active 态用 `bg-primary/8` 而非 `bg-secondary/70`，更强调"动作入口"。
- 左侧加 download icon。

### `CATEGORIES` 数据结构变更

```ts
// 主类目（项目内资产）
const PRIMARY_CATEGORIES: CategoryItem[] = [
  { segment: "workflow", label: "Workflow", hint: "Lifecycle + Workflow 模板" },
  { segment: "canvas", label: "Canvas", hint: "可视化资产" },
  { segment: "mcp-preset", label: "MCP Preset", hint: "MCP Server 模板" },
  { segment: "skill", label: "Skill", hint: "Agent 可读技能包" },
];

// 安装入口（仅一项，但用同一组件保证未来可扩展）
const SOURCE_CATEGORIES: CategoryItem[] = [
  { segment: "marketplace", label: "资源市场", hint: "从公共库安装资产" },
];
```

`<NavItem>` 组件签名：

```ts
interface NavItemProps {
  to: string;
  label: string;
  hint: string;
  variant: "primary" | "source";       // primary = 主类目, source = 入口
}
```

variant 内嵌样式表（不传 className 拼接），保持调用点干净。

## 关键 Tradeoff

1. **flatten + group vs. 维持 5 数组**：选 flatten。代价是失去"哪个 kind 装在哪"的层
   级感，由 chip tooltip 列出弥补。收益是卡片状态权威化、底部冗余区可去。
2. **抽屉 vs. modal**：选右侧抽屉。modal 会遮挡卡片网格，用户从一张卡片快速看下一
   张时不得不连续 open/close；抽屉允许 grid 保留滚动状态，且支持后续做"上一条/下一条"
   导航（不在本轮范围）。
3. **search 前端过滤 vs. 后端 query**：选前端过滤。资产规模 < 100 时无性能压力，避免
   每按一键都打后端；类型筛选保留服务端，便于以后做分页。
4. **payload 类型不收敛**：本轮不引入 TS 类型，drawer 用宽松解析 + fallback。后续可
   开任务把 `LibraryAssetDto.payload` 类型化。
5. **Notice 抽出粒度**：放 `_shared/` 而非 `components/`。理由：当前 `assets-panel`
   是唯一消费者；过早全局化是 [workflow_design_principle] 提到的过度抽象。
6. **侧栏底部固定 vs. 顶部独立组**：选底部固定 + `mt-auto`。底部贴边强调"辅助入口"
   语义，符合常见 IDE/dashboard 把 settings/help 之类放底部的惯例；顶部独立组也合理
   但视觉权重过高，会与主类目竞争。
