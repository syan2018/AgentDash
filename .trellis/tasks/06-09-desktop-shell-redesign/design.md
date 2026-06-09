# Design — 桌面外壳设计重塑与错误体系

承接 `prd.md`。本文聚焦技术设计：边界、契约、数据流、取舍、兼容与回滚形态。

## 0. 架构总览

三个改动面，依赖自下而上：

```
设计基座 (packages/ui)         ← R1 token/primitive + R2.2 StatusScreen
   │
   ├── app-web (错误体系/状态屏复用)   ← R2 ErrorBoundary + 状态屏接线
   │
   └── app-tauri + crate (外壳/splash) ← R3 titlebar + 窗口效果 + splash
```

落点先后：**先基座（token + StatusScreen），再 app-web 错误体系，最后 Tauri 外壳/splash**。基座是 splash 与错误屏的共同依赖，必须先稳定。

---

## 1. R1 设计语言演进

### 1.1 Elevation / Surface 取舍

现状 spec：depth 严格三档（0 background / 1 card+border / 2 secondary），**无 shadow**，且"嵌套二选一"。本次演进的核心决策：

- **引入受控的阴影 token**，但不推翻三档 surface——阴影是"depth 的第二信号"，与 bg/border 协同而非替代。新增（写入 `styles.css` `@theme inline` 或 `:root` 自定义属性）：
  - `--shadow-sm`：卡片静置（极淡，1–2px 模糊，低 alpha）。
  - `--shadow-md`：浮起态 / popover / dropdown。
  - `--shadow-lg`：dialog / 抽屉。
  - 暗色模式下阴影 alpha 提高、用更深的投影色（暗色里阴影本就更难感知，靠 border 高光 + 深投影补）。
- **修订"嵌套二选一"规则**：原规则禁止"边框 + 填底 + 圆角"三者叠加。新规则：允许"填底 + 圆角 + 阴影"组合（阴影替代边框承担分隔），或"描边 + 圆角"组合；**阴影与重边框二选一**，避免又描边又投影的廉价感。这条要明确写进 design-language.md。
- Tailwind v4 用法：`shadow-[var(--shadow-sm)]` 或在 `@theme inline` 暴露为 `--shadow-*` 让 `shadow-sm` 等类直接可用（优先后者，保持 utility 一致性）。

### 1.2 Sidebar 独立表面

- sidebar 从 `bg-background` 改为**独立表面色**。两种候选（design.md 选 A，B 留作 fallback）：
  - **A（推荐）**：sidebar = `bg-card`（depth-1）或新增的极淡 tint surface，内容区保持 `bg-background`（depth-0），右缘用 `--shadow-sm`（inset 或右投影）替代 1px border。形成"内容区是工作台、sidebar 是固定面板"的纵深。
  - B：sidebar 维持同色但加 inset 顶部高光 + 略深右边框——更保守，观感提升有限。
- 注意 light/dark 都要校准：dark 模式下 `card` 比 `background` 略亮，sidebar 用 card 会"浮起"，需确认方向感正确（侧栏通常应"沉"或"稳"，必要时给 sidebar 单独的 surface 变量而非复用 card）。
- 风险：sidebar 内大量子元素（ProjectDropdown、NavLink、SessionShortcutList、SidebarFooter）当前按 `bg-background` 设计，换底色后这些元素的 hover/active 态对比需逐个回归（见 implement.md 校验项）。

### 1.3 按钮：维持描边，不实心化（决策修订 2026-06-09）

- 初版计划把 primary 改实心，G1 评审时确认"满饱和蓝实心填充太不克制"，与 design-language §"饱和度克制、避免强彩"冲突。**决策：撤销实心化，按钮维持原描边风格**，`design-language.md` §6"空心边框风格"不变。
- "去单薄"完全由 elevation（§1.1）+ sidebar 独立表面（§1.2）承担，不靠按钮受重。
- 蓝色（`--primary`）保持仅用于链接 / 焦点 / 少量强调，不做大面积实心填充。
- active nav 当前在 `workspace-layout.tsx` 是 `bg-primary` 大蓝块（既有代码），同属"不克制"问题，将在 P3 一并改为克制的选中态（neutral 填充 / 描边强调），改前与用户确认。

### 1.4 设计语言文档与预览（治理）

- 更新 `.trellis/spec/frontend/design-language.md`：第 2 节加 shadow token 行、第 4 节 surface 加"阴影作为 depth 第二信号"与修订后的嵌套规则、第 6 节 Button 描述从"空心边框风格"改为"主操作实心、其余描边"。
- `DesignSystemPage`（`/dev/design-system`）补：elevation 三档预览块、sidebar-vs-content 表面对比块、Button 新形态。该页是 AC 的**视觉验收基准**。

---

## 2. R2 错误体系

### 2.1 组件边界

新增/改造（都在 `packages/app-web/src`，状态屏 primitive 放 `packages/ui` 以便 app-tauri 也能用）：

| 组件 | 位置 | 职责 |
|------|------|------|
| `StatusScreen`（primitive） | `packages/ui/src/primitives/StatusScreen.tsx` | 纯展示：icon/spinner + 标题 + 描述 + 可选 action 按钮 + tone（loading/info/warning/danger）。无业务逻辑。 |
| `AppErrorBoundary` | `packages/app-web/src/components/error/AppErrorBoundary.tsx` | class 组件，`getDerivedStateFromError` 存错误，`componentDidCatch` 记录；fallback 渲染 `StatusScreen`（崩溃态）+ "重载应用"按钮（`window.location.reload()`）。支持可选 `onReset`/`resetKeys` 以做路由级可恢复。 |
| 接线 | `app-web/src/App.tsx` / `WorkspaceLayout` | 顶层包 `AppErrorBoundary`；`WorkspaceLayout` 的 `<Outlet>` 外再包一层带 `resetKeys={[location.pathname]}` 的 boundary（路由切换自动恢复）。 |

### 2.2 复用归一

- `RouteFallback`（App.tsx 内）→ 改为 `StatusScreen` loading 态。
- `BootstrapErrorState`（App.tsx 内）→ 改为 `StatusScreen` danger 态 + 重试 action。
- 这样 splash / loading / bootstrap-error / route-fallback / crash 全部一套视觉语言，dark/light 一致（AC2/AC6）。

### 2.3 为什么不用 react-router `errorElement`

`errorElement` 仅在 data router（`createBrowserRouter`）下可用。当前是 `BrowserRouter` + JSX 路由表，迁移成本与回归面大（C5）。React ErrorBoundary 组件即可覆盖渲染期崩溃，且能放在任意层级做"整壳兜底 + 路由级可恢复"，更贴合现状。异步/fetch 错误本就由各 store/page 的状态处理，不在 ErrorBoundary 职责内（boundary 只兜 render-throw）。

### 2.4 边界与限制

- ErrorBoundary 兜不住：事件回调里的异步错误、`useEffect` 里的 promise rejection——这些维持现有 per-store 错误态。本任务不扩张到全局 toast 体系（Out of Scope 之外的克制）。

---

## 3. R3 桌面外壳 + splash

### 3.1 自定义 titlebar 归属

- titlebar 必须横跨整窗（在 sidebar + 内容之上）。归属决策：**由 app-tauri 宿主在 `WebDashboardApp` 外层渲染一条 `<DesktopTitlebar>`**，而不是塞进 app-web 的 WorkspaceLayout（避免污染 Web 形态、避免 layout 改动）。
  - app-tauri `App.tsx` 结构变为：`<TauriShell>`（titlebar + 下方 `WebDashboardApp`）。
  - app-web 整体高度从 `h-screen` 改为填充父容器（`h-full`），由 tauri 宿主用 `flex-col` 预留 titlebar 高度。需确认 app-web 在 Web 形态下父容器仍是全屏（main.tsx / index.html 根容器给 `h-screen`）。
- `DesktopTitlebar` 用 `@tauri-apps/api/window` 的 `getCurrentWindow()`：`.minimize()` / `.toggleMaximize()` / `.close()`，标题区 `data-tauri-drag-region`。组件可放 `packages/views` 或 app-tauri 本地（倾向 app-tauri 本地，因强 Tauri 耦合）。

### 3.2 Capabilities

- `decorations:false` 后窗口控制需权限。在 Tauri capabilities（`crates/agentdash-local-tauri` 的 capabilities 配置 / `tauri.conf.json` 引用）加：`core:window:allow-minimize`、`core:window:allow-toggle-maximize`（或 maximize/unmaximize）、`core:window:allow-close`、`core:window:allow-start-dragging`。
- 现有 `gen/schemas/capabilities.json` 是生成物，需确认源 capabilities 文件位置（implement.md 第一步勘定）。

### 3.3 窗口效果策略（C1 Win10）

- **不依赖 Mica**。方案：
  - 默认：titlebar + 窗体为**实色 token 表面**（与新 surface 体系一致），不开任何 effect——这是基线，保证 Win10 不卡、不破相。
  - 可选增强（后续/降级开关）：若要 Acrylic，用 `window-vibrancy` crate 在 setup 里按 OS 版本探测启用，Win10 失败则静默回落实色。**本任务默认不引入 window-vibrancy 依赖**，先做实色 titlebar；effect 作为 stretch，不阻塞 AC。
- 结论：AC5 通过的判据是"无 Mica 依赖 + 实色表面不卡顿"，effect 是加分项不是必选。

### 3.4 splash 重做

- `DashboardHost`（app-tauri/App.tsx）保留其状态机（`checking`/`ready`/`unavailable` + snapshot 轮询，C3），仅把渲染分支从旧 `Card` 换成 `StatusScreen`：
  - `checking`/starting → loading 态（品牌 logo + spinner + 进度文案）。
  - `unavailable` → danger 态 + 重试。
  - `ready` → 渲染 `WebDashboardApp`。
- splash 出现在 titlebar 之下（titlebar 始终可见、可拖拽，即使后端没起来用户也能拖窗/关窗——体验提升点）。

---

## 4. 数据流 / 兼容

- 无新增后端接口、无 DTO 变更、无数据库改动。纯前端 + Tauri 配置/壳层。
- `desktop_api_snapshot` / `/api/health` 契约不变（C3）。
- Web 形态（无 `__TAURI_INTERNALS__`）：不渲染 titlebar、不调 window API、布局回退全屏——需显式条件分支并测试（AC4）。

## 5. 回滚形态

按 commit 粒度分三段，任一段可独立回退：

1. **R1 基座**：token/primitive 改动集中在 `packages/ui` + design-language.md + DesignSystemPage。回退 = revert 该 commit，业务页面回到旧观感，无功能影响。
2. **R2 错误体系**：新增组件 + App.tsx 接线。回退 = 移除 boundary 包裹，回到"无兜底"现状（白屏），不影响正常路径。
3. **R3 外壳**：`tauri.conf.json` `decorations` + titlebar + capabilities + splash 换皮。回退 = `decorations` 改回默认 + 还原 App.tsx，回到系统标题栏 + 旧 splash 逻辑（逻辑本就没动，安全）。

## 6. 关键取舍小结

| 取舍 | 选择 | 理由 |
|------|------|------|
| 阴影 vs 纯边框 depth | 引入受控阴影，修订嵌套规则 | 去"单薄"的最直接手段，但写回 spec 保持治理 |
| 主按钮实心 vs 空心 | 维持空心描边 | G1 评审：实心蓝太不克制；depth 由 elevation+sidebar 承担 |
| titlebar 归属 | app-tauri 宿主层 | 隔离 Web 形态，零侵入 app-web layout |
| errorElement vs ErrorBoundary | ErrorBoundary 组件 | 避免 data-router 迁移风险（C5） |
| Mica/Acrylic vs 实色 | 实色为基线，effect 为 stretch | Win10 无 Mica、Acrylic 卡顿（C1） |
