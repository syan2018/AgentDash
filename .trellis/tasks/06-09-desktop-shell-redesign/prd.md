# 桌面外壳设计重塑与错误体系

## Goal

把 AgentDash 桌面端从"网页钉进系统窗口"的单薄/板正观感，中度重塑为一个有纵深、有品牌、有完整错误兜底的桌面应用外壳。三条主线：

1. **设计语言演进**：引入 elevation/surface 纵深、让 sidebar 成为独立表面、主按钮改实心受重面——并把这些写回 `design-language.md` 与 `DesignSystemPage`。
2. **启动 splash**：把 `DashboardHost` 的健康检查小卡片重做成带品牌的启动屏，保留其 `/api/health` gate 契约。
3. **错误体系**：建立顶层 ErrorBoundary + 统一状态屏组件，消除白屏与裸网页报错。

## Background（为什么做）

现状定位（已核实，非主观）：

- **设计层**：`packages/ui/src/styles.css` 整体是"灰底 + 1px 淡边框分隔"，`--background:98%` / `--card:100%` / `border:220 14% 91%`（极淡），sidebar 用 `bg-background`（与内容区同色）仅靠 `border-r` 分隔；无 shadow token；主按钮为透明描边（`Button` variant `primary`）。这套观感有**相当一部分是 `design-language.md` 明文规定的**（surface 三档、禁 depth-3、嵌套二选一、按钮"空心边框风格"）。因此本任务是**有意识地演进设计语言**，不是绕开它。
- **外壳层**：`crates/agentdash-local-tauri/tauri.conf.json` 未设 `decorations`（=系统默认标题栏），无 window effect，单 icon；第一眼是 `DashboardHost`（`packages/app-tauri/src/App.tsx`）的健康检查小卡片，无品牌。
- **错误层**：全项目 grep 不到任何 ErrorBoundary / `componentDidCatch` / 路由 `errorElement`。React 渲染期抛错 = 白屏；只有 `AuthGate` 里的 `BootstrapErrorState` 兜了"身份初始化失败"一种情况。

## Constraints（硬约束）

- **C1 Windows 10**：开发/目标机为 Win10（19045）。Mica 是 Win11 限定，**不能依赖 Mica**；Win10 仅 Acrylic 可用且有拖动卡顿已知问题。窗口效果策略必须可优雅降级（无 effect 时是实色表面，不破相）。
- **C2 app-web 同时是 Web 目标**：`packages/app-web` 也作为浏览器应用发布。自定义 titlebar / 窗口控制按钮**只能在 Tauri 宿主内生效**（`window.__TAURI_INTERNALS__` 探测），不得影响 Web 形态。
- **C3 保留 DashboardHost gate 契约**：`desktop-local-runtime.md` 规定"必须先确认 `/api/health` ready 才渲染 Dashboard"、"API 尚未启动展示 starting 并轮询"、"`/api/health` 非 2xx 不渲染"。splash 只换视觉，不改这套状态机语义。
- **C4 设计语言是受治理的 spec**：任何 token/surface/primitive/radius 改动必须同步更新 `design-language.md` 第 2/4/6 节，并在 `DesignSystemPage`（`/dev/design-system`）补预览；新代码 0 ESLint warning（无字面色、无任意半径）。
- **C5 不做 react-router data-router 迁移**：当前是 `BrowserRouter` + element 路由。错误兜底用 React ErrorBoundary 组件实现，不强行迁移到 `createBrowserRouter` 以换取 `errorElement`（风险过大、收益有限）。

## Requirements

### R1 设计语言演进（中度重塑）
- R1.1 引入 **elevation token**（如 `--shadow-sm/md/lg` 或等价的 surface 分层规则），让 sidebar、card、popover 有可区分的纵深；明确"何时用阴影、何时用边框"，更新"嵌套二选一"与三档 surface 规则使之自洽。
- R1.2 **sidebar 成为独立表面**：与内容区拉开（独立底色/极淡 tint 或 inset/右侧阴影收边），不再是同色 + 1px 边框。
- R1.3 ~~主按钮改实心~~ **（2026-06-09 撤销）**：G1 评审确认实心蓝不克制，按钮维持原描边风格；去单薄由 elevation + sidebar 承担。`--primary` 大面积实心填充（含 active nav 大蓝块）一并收敛为克制用色。
- R1.4 所有改动落到 `packages/ui/src/styles.css` + 相关 primitive，并更新 `design-language.md` + `DesignSystemPage`。
- R1.5 light + dark 双模式均需校准（`use-theme` 已支持 system/light/dark）。

### R2 错误体系
- R2.1 新增顶层 **ErrorBoundary**（class 组件，`getDerivedStateFromError` + `componentDidCatch`），包裹应用主体，捕获 React 渲染期崩溃，展示品牌化崩溃屏（含错误摘要 + 重载/重试）。
- R2.2 抽出**统一状态屏 primitive**（暂名 `StatusScreen`/`AppStateScreen`）：覆盖 loading / api-unavailable / 离线 / 401 / 崩溃 等态，供 splash、AuthGate、ErrorBoundary、route fallback 复用，替换现有 `RouteFallback` 与 `BootstrapErrorState` 的散装实现。
- R2.3 路由级兜底：在 `WorkspaceLayout` 的 `<Outlet>` 外再包一层 boundary，单页面崩溃不拖垮整壳（可重置）。
- R2.4 错误屏文案与按钮走设计 token，dark/light 一致。

### R3 桌面外壳 + 启动 splash
- R3.1 `tauri.conf.json` 设 `decorations: false`，实现**自定义 titlebar**：可拖拽区（`data-tauri-drag-region`）+ 自绘窗口控制（最小化/最大化-还原/关闭，调 `@tauri-apps/api/window`），并补齐 Tauri capabilities 中对应 window 权限。
- R3.2 titlebar 仅 Tauri 宿主渲染（C2）；与 sidebar 品牌头/项目切换/连接状态在视觉上协调成"窗口的一部分"。
- R3.3 **窗口效果策略**：尝试在支持的系统上启用窗口效果，Win10/不支持时降级为实色表面，保证不破相、不卡顿（C1）。
- R3.4 把 `DashboardHost` 健康检查卡片重做成**品牌 splash**：复用 R2.2 的 StatusScreen，保留 `/api/health` gate 状态机（C3）。

## Acceptance Criteria

- [ ] AC1 桌面端启动呈现品牌 splash（非旧灰卡片），API 未就绪时按 starting/轮询/不可用三态正确展示，ready 后渲染 Dashboard——`/api/health` gate 语义不变。
- [ ] AC2 手动在某页面 throw 一个错误，应用展示品牌崩溃屏并可重载/重置，**不再白屏**；崩溃不冒泡成裸浏览器报错。
- [ ] AC3 sidebar 与内容区视觉上分属不同表面层；card/popover 有可感知纵深——light + dark 双模式均成立。蓝色用色克制（无大面积实心蓝块）。
- [ ] AC4 自定义 titlebar 在 Tauri 下可拖拽、窗口控制按钮（最小化/最大化/关闭）功能正常；在 Web 形态下不出现 titlebar、布局无异常。
- [ ] AC5 Win10 上无 Mica 依赖；窗口效果缺失时为实色表面，无拖动卡顿。
- [ ] AC6 `design-language.md`（token/surface/primitive 章节）与 `DesignSystemPage` 已同步更新；触达的 UI 代码 0 ESLint warning。
- [ ] AC7 typecheck / lint / build 通过；关键路径有 Playwright 烟测（splash → ready、错误屏出现）。

## Out of Scope

- 信息架构 / 导航结构调整（保持 Agent/Story/Assets/Routine 四区与现有路由）。
- 大胆重做级别的配色/品牌重定义（本次为"中度重塑"）。
- macOS / Linux 的原生 titlebar 适配细节（以 Windows 为主，跨平台保持可用即可，不在本次精修）。
- react-router 数据路由迁移（见 C5）。
