---
name: Session 右栏浏览器化动态 Tab 系统
overview: |
  将 SessionPage 右栏 WorkspacePanel 从固定 Tab 架构演进为浏览器风格的动态 Tab 系统。
  支持 Tab 实例的动态添加/关闭/拖拽排序，引入 URI scheme 标识每个 Tab 目标，
  VFS 浏览器升级为左侧文件树+右侧 CodeMirror 编辑器的双栏布局，
  并预留 TabType 注册接口以支持未来的插件扩展。
todos:
  - id: tab-type-registry
    content: "阶段1: 设计 TabTypeDescriptor 接口和 TabTypeRegistry 注册机制"
    status: pending
  - id: tab-store
    content: "阶段2: 创建 useWorkspaceTabStore (Zustand)，管理 Tab 实例生命周期"
    status: pending
  - id: workspace-panel-v2
    content: "阶段3: 重构 WorkspacePanel 为动态 Tab UI（钉选Tab + 动态Tab + 地址栏 + 新建菜单）"
    status: pending
  - id: tab-dnd
    content: "阶段4: 使用 dnd-kit 实现动态 Tab 拖拽排序"
    status: pending
  - id: vfs-browser-v2
    content: "阶段5: VFS 浏览器升级为文件树+CodeMirror 编辑器双栏布局"
    status: pending
  - id: terminal-placeholder
    content: "阶段6: 注册 Terminal Tab 类型，展示占位符"
    status: pending
  - id: backend-persist
    content: "阶段7: Tab 布局持久化到后端 session meta"
    status: pending
  - id: session-page-integration
    content: "阶段8: SessionPage 集成 — 事件驱动 Tab 创建、状态迁移"
    status: pending
  - id: cleanup-verify
    content: "阶段9: 清理废弃代码、typecheck、lint 验证"
    status: pending
isProject: false
---

# Session 右栏浏览器化动态 Tab 系统

## 背景

当前 `WorkspacePanel` 右栏有 4 个固定 Tab（上下文、地址空间、Canvas、审计），
类型硬编码为 `"context" | "vfs" | "canvas" | "inspector"` 联合类型。
每种 Tab 仅允许一个实例，无法同时浏览多个 Canvas 或多个 VFS mount。
VFS 浏览器仅提供简单的文件列表+预览面板，缺少编辑器级别的文件浏览体验。

参考 Cursor IDE 的面板系统（浏览器风格多 Tab + 新建菜单），
本次重构目标是建立一个可扩展的动态 Tab 系统。

## 设计决策记录

| 决策项 | 选择 | 理由 |
|--------|------|------|
| 固定 Tab 处理 | 上下文/审计保留为钉选 Tab（不可关闭），动态 Tab 在其右侧 | 这两个 Tab 是会话必备上下文 |
| 多实例策略 | 按类型区分：Canvas/VFS 允许多实例，上下文/审计单实例 | 多 Canvas/多 mount 是实际使用场景 |
| VFS 编辑器 | CodeMirror 6 | 轻量、现代、适合嵌入，语法高亮充足 |
| 插件范围 | 仅 TabType 注册接口 | 定义好 Descriptor 数据结构，不做运行时加载 |
| URL 标识 | 自定义 scheme URI | `canvas://{canvasId}`, `vfs://{surfaceRef}/{mountId}/{path}`, `terminal://{termId}` |
| Tab 持久化 | 后端存储 | 作为 session meta 的一部分持久化到服务端 |
| 状态管理 | Zustand Store (useWorkspaceTabStore) | 项目一致性，跨组件访问，middleware 持久化 |
| 拖拽排序 | dnd-kit（已有依赖） | 项目已安装 @dnd-kit/core + @dnd-kit/sortable |
| VFS 文件树 | Mount 列表 + 懒加载子树 | 支持企业知识库等大型 mount 场景 |
| Tab 数量限制 | 不限制 | Tab 条溢出时滚动处理 |
| Canvas 事件行为 | 复用或新建 | `canvas_presented` 先查找同 canvasId Tab，有则激活，无则新建 |
| Terminal | 仅注册类型+占位符 | 终端执行完整支持由独立任务追踪 |

## 核心数据模型

### TabTypeDescriptor — Tab 类型描述符（插件注册接口）

```typescript
interface TabTypeDescriptor {
  /** 类型唯一标识，如 "canvas", "vfs", "terminal" */
  typeId: string;
  /** 显示名称 */
  label: string;
  /** Tab 栏中的图标（React 组件） */
  icon: React.ComponentType<{ className?: string }>;
  /** 是否允许多实例 */
  allowMultiple: boolean;
  /** 是否为钉选 Tab（始终存在，不可关闭） */
  pinned: boolean;

  /** 渲染 Tab 内容区域 */
  renderContent: (props: TabContentRenderProps) => React.ReactNode;

  /** 从 URI 解析出 Tab 标题 */
  resolveTitle: (uri: string) => string;
  /** 从 URI 解析出参数 */
  parseUri: (uri: string) => Record<string, string> | null;
  /** 从参数构建 URI */
  buildUri: (params: Record<string, string>) => string;

  /** 新建 Tab 时的默认 URI（可选，用于 "+" 菜单直接创建） */
  defaultUri?: string;
  /** "+" 菜单中的排序权重 */
  menuOrder?: number;
}

interface TabContentRenderProps {
  uri: string;
  tabId: string;
  sessionId: string | null;
  isActive: boolean;
}
```

### TabInstance — Tab 实例

```typescript
interface TabInstance {
  /** 唯一实例 ID */
  id: string;
  /** 引用 TabTypeDescriptor.typeId */
  typeId: string;
  /** 标识此 Tab 目标的 URI */
  uri: string;
  /** 显示标题（可由用户修改或自动解析） */
  title: string;
  /** 是否为钉选 Tab */
  pinned: boolean;
}
```

### URI Scheme 规范

| Tab 类型 | URI 格式 | 示例 |
|----------|----------|------|
| 上下文 | `context://overview` | `context://overview` |
| 审计 | `inspector://session/{sessionId}` | `inspector://session/abc123` |
| Canvas | `canvas://{canvasId}` | `canvas://cv_9f3a2b` |
| VFS | `vfs://{surfaceRef}/{mountId}?path={path}` | `vfs://sf_abc/mount_1?path=src/main.rs` |
| Terminal | `terminal://{terminalId}` | `terminal://term_001` |

## 架构设计

### 组件层次

```
SessionPage
├── Header（不变）
└── PanelGroup (horizontal)
    ├── Panel: SessionChatView（不变）
    ├── Separator（不变）
    └── Panel: WorkspacePanel v2
        ├── TabBar
        │   ├── PinnedTabs (上下文 / 审计)
        │   ├── DynamicTabs (Canvas / VFS / Terminal ...)
        │   │   └── SortableContext (dnd-kit)
        │   └── AddTabButton (+)
        │       └── Dropdown: 可添加的 Tab 类型列表
        ├── AddressBar (显示当前 Tab 的 URI / 可读标签)
        └── TabContent
            ├── ContextOverviewTab（钉选）
            ├── ContextInspectorPanel（钉选）
            ├── CanvasSessionPanel（动态，多实例）
            ├── VfsBrowserV2（动态，多实例）
            │   ├── Left: MountSelector + FileTree (懒加载)
            │   └── Right: CodeMirror 6 Editor
            └── TerminalPlaceholder（动态）
```

### Zustand Store — useWorkspaceTabStore

```typescript
interface WorkspaceTabState {
  /** 所有 Tab 实例（钉选在前，动态在后） */
  tabs: TabInstance[];
  /** 当前激活的 Tab ID */
  activeTabId: string | null;
  /** 当前 session ID（用于持久化关联） */
  sessionId: string | null;

  // ── Actions ──
  /** 初始化（从后端恢复或默认状态） */
  initialize: (sessionId: string | null, saved?: TabInstance[]) => void;
  /** 添加新 Tab 实例 */
  addTab: (typeId: string, uri: string, activate?: boolean) => string;
  /** 关闭 Tab */
  closeTab: (tabId: string) => void;
  /** 激活 Tab */
  activateTab: (tabId: string) => void;
  /** 按 URI 查找并激活，不存在则新建 */
  openOrActivate: (typeId: string, uri: string) => string;
  /** 拖拽排序后更新顺序 */
  reorderTabs: (fromId: string, toId: string) => void;
  /** 更新 Tab URI（导航到新位置） */
  updateTabUri: (tabId: string, uri: string) => void;
  /** 持久化到后端 */
  persistToBackend: () => Promise<void>;
}
```

### TabTypeRegistry — 全局注册表

```typescript
class TabTypeRegistry {
  private types = new Map<string, TabTypeDescriptor>();

  register(descriptor: TabTypeDescriptor): void;
  unregister(typeId: string): void;
  getType(typeId: string): TabTypeDescriptor | undefined;
  listTypes(): TabTypeDescriptor[];
  listCreatableTypes(): TabTypeDescriptor[]; // 排除 pinned
}

// 全局单例
export const tabTypeRegistry = new TabTypeRegistry();
```

内置注册（在应用初始化时执行）：

```typescript
tabTypeRegistry.register(contextTabType);   // pinned
tabTypeRegistry.register(inspectorTabType); // pinned
tabTypeRegistry.register(canvasTabType);    // dynamic, multi
tabTypeRegistry.register(vfsTabType);       // dynamic, multi
tabTypeRegistry.register(terminalTabType);  // dynamic, multi, placeholder
```

## VFS 浏览器 v2 设计

### 布局

```
┌──────────────────────────────────────────────┐
│ [Mount: workspace] ▼  | 搜索: [________]     │  ← Mount 选择器 + 搜索
├──────────┬───────────────────────────────────┤
│ 📁 src   │  // main.rs                       │
│  📁 api  │  fn main() {                      │
│  📁 db   │      let app = create_app();      │
│  📄 lib  │      app.run().await;             │
│ 📁 tests │  }                                │
│ 📄 Cargo │                                   │
│ 📄 README│                                   │
├──────────┤  ← react-resizable-panels 分隔    │
│          │                                   │
└──────────┴───────────────────────────────────┘
     ↑ 懒加载文件树               ↑ CodeMirror 6 编辑器
```

### 关键实现点

- **文件树**：顶部 Mount 下拉选择器，下方为当前 mount 的懒加载目录树
- **懒加载**：展开目录节点时调用 `listSurfaceMountEntries`，首次加载当前目录层
- **CodeMirror 6**：需要安装 `@codemirror/view`, `@codemirror/state`, `@codemirror/lang-*` 等
- **读写**：复用现有 `readSurfaceFile` / `writeSurfaceFile` API
- **左右分栏**：使用已有的 `react-resizable-panels` 实现可调整分隔

## Terminal Tab 占位

- 注册 `terminal` TabTypeDescriptor
- URI scheme: `terminal://{terminalId}`
- 内容区显示占位符："终端功能即将支持，敬请期待"
- 后续由独立任务实现完整终端前后端通道

## 后端持久化

Tab 布局数据结构（存入 session meta）：

```typescript
interface SessionTabLayout {
  tabs: Array<{
    type_id: string;
    uri: string;
    title: string;
    pinned: boolean;
  }>;
  active_tab_uri: string | null;
}
```

- 在 Tab 状态变更（添加/关闭/激活/排序）后防抖写入
- 进入 SessionPage 时从 session meta 恢复 Tab 布局
- 新会话默认只有两个钉选 Tab

## 涉及文件变更

| 操作 | 文件 |
|------|------|
| 新建 | `features/workspace-panel/tab-type-registry.ts` — Tab 类型注册表 |
| 新建 | `features/workspace-panel/tab-types/` — 各内置 Tab 类型定义目录 |
| 新建 | `features/workspace-panel/tab-types/context-tab.tsx` |
| 新建 | `features/workspace-panel/tab-types/inspector-tab.tsx` |
| 新建 | `features/workspace-panel/tab-types/canvas-tab.tsx` |
| 新建 | `features/workspace-panel/tab-types/vfs-tab.tsx` |
| 新建 | `features/workspace-panel/tab-types/terminal-tab.tsx` |
| 新建 | `stores/workspaceTabStore.ts` — Zustand Tab 状态管理 |
| 新建 | `features/workspace-panel/TabBar.tsx` — Tab 栏 UI（含拖拽） |
| 新建 | `features/workspace-panel/AddressBar.tsx` — URI 地址栏 |
| 新建 | `features/workspace-panel/AddTabMenu.tsx` — "+" 新建菜单 |
| 重构 | `features/workspace-panel/WorkspacePanel.tsx` — 主容器重构 |
| 重构 | `features/workspace-panel/workspace-panel-types.ts` — 类型更新 |
| 重构 | `features/workspace-panel/index.ts` — 导出更新 |
| 重构 | `features/vfs/vfs-browser.tsx` — 升级为双栏布局 |
| 新建 | `features/vfs/vfs-file-tree.tsx` — 懒加载文件树组件 |
| 新建 | `features/vfs/vfs-code-editor.tsx` — CodeMirror 编辑器封装 |
| 重构 | `pages/SessionPage.tsx` — 集成 Tab Store，迁移事件处理 |
| 小改 | `services/session.ts` — Tab 布局持久化 API |

## 不变的部分

- `SessionChatView` 内部结构和 Props 不变
- 左栏导航不变
- `CanvasSessionPanel` / `CanvasRuntimePreview` 内部逻辑不变
- `ContextOverviewTab` 内部逻辑不变
- `ContextInspectorPanel` 内部逻辑不变
- 现有路由结构不变
- 后端 VFS API 不变

## 插件扩展使用示例

企业定制场景：

```typescript
import { tabTypeRegistry } from "@/features/workspace-panel";

tabTypeRegistry.register({
  typeId: "knowledge-base",
  label: "知识库",
  icon: BookIcon,
  allowMultiple: true,
  pinned: false,
  renderContent: ({ uri, sessionId }) => (
    <KnowledgeBaseBrowser uri={uri} sessionId={sessionId} />
  ),
  resolveTitle: (uri) => `知识库: ${parseKbName(uri)}`,
  parseUri: (uri) => ({ kbId: uri.replace("kb://", "") }),
  buildUri: ({ kbId }) => `kb://${kbId}`,
  menuOrder: 50,
});
```
