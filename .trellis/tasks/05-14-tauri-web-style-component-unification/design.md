# 设计：Tauri 桌面端与 Web 前端样式和组件边界统一

## 背景与问题判断

当前 Tauri 桌面端不是单纯 WebView 渲染差异，而是宿主入口与样式契约不一致：

1. `app-tauri` 渲染 `WebDashboardApp`，但其 Tailwind 构建上下文与 `app-web` 不完全一致，导致桌面端 CSS 产物缺少大量 Web Dashboard 使用的 utility。
2. `@agentdash/views/local-runtime.css` 在全局 `:root` 上覆盖 `--border`、`--primary`、`--muted` 等变量，而 Web Tailwind 主题使用 `hsl(var(--border))` 这类 HSL token。HEX 值覆盖 HSL token 后会让一批颜色规则失效。
3. `app-web` 和 `app-tauri` 入口导入的全局样式集合不同，`streamdown` 与 `katex` 样式当前只在 Web 入口显式导入。
4. Local Runtime 视图 CSS 同时定义桌面 shell、DashboardHost、runtime panel 等样式，作用域不清，容易污染 Web Dashboard。

因此本任务的核心不是“给 Tauri 另写一套样式”，而是建立一个宿主无关的设计系统包，并让 Web 与 Tauri 共同消费同一套视觉契约。

本任务不按技术债兼容路线设计。目标终态是唯一设计系统、唯一全局 CSS 入口、业务 views 无全局 CSS 副作用、宿主只负责装配和平台能力适配。

## 目标架构

### 包职责

| Package | 职责 | 不应承担 |
| --- | --- | --- |
| `packages/ui` / `@agentdash/ui` | 设计系统事实源；Tailwind v4 CSS 入口、主题 token、base styles、基础 UI primitives、共享 component class | 业务数据访问、Tauri command、页面路由 |
| `packages/app-web` | 浏览器宿主、React Router、认证入口、Web Dashboard 装配 | Tauri command 适配、本机进程管理、跨包样式事实源 |
| `packages/app-tauri` | 桌面宿主、Tauri navigation shell、DashboardHost 健康检查、Tauri command -> port adapter | 复制 Web Dashboard 组件树、覆盖 Web 主题 token |
| `packages/views` | 可跨宿主复用的业务视图，如 Local Runtime UI；后续逐步承载 Dashboard 可复用视图 | 全局宿主样式、浏览器/Tauri 专属路由 |
| `packages/core` | 无头类型、端口、纯函数、local runtime client contract | React UI、CSS、Tauri API 直接调用 |

这个边界延续现有 `Desktop Local Runtime` 契约：Dashboard 数据 authority 仍是 HTTP API；Local Runtime 设置才通过 Tauri command 访问本机 runtime manager。

### 样式契约：采用 `@agentdash/ui`

更标准的一步到位方案是新增 `packages/ui`，而不是继续让 `app-web` 暂时代管全局样式。目标结构：

```text
packages/ui/
  package.json            # name: @agentdash/ui
  src/
    styles.css            # 唯一全局 Tailwind / theme / base / components 入口
    tokens.css            # 可选：纯 CSS token，供非 Tailwind 场景复用
    primitives/           # Button / Input / Select / Textarea / Card / Badge 等基础组件

packages/app-web/src/styles/index.css
  -> @import "@agentdash/ui/styles.css";

packages/app-tauri/src/main.tsx
  -> import "@agentdash/ui/styles.css";
```

`@agentdash/ui/styles.css` 必须包含：

- Tailwind v4 import。
- Web Dashboard 主题 token。
- Tailwind `@source`，覆盖：
  - `packages/app-web/src/**/*.{ts,tsx}`
  - `packages/app-tauri/src/**/*.{ts,tsx}`
  - `packages/views/src/**/*.{ts,tsx}`
  - 需要扫描的第三方渲染包。
- Dashboard 必需全局 CSS：
  - `streamdown/styles.css`
  - `katex/dist/katex.min.css`
  - 其它实际由 Dashboard 组件依赖的包级 CSS。

Web 入口和 Tauri 入口只应导入这份样式契约，不再维护两份全局样式清单。`app-web/styles.css` 不再作为跨包样式入口保留；`app-tauri` 直接依赖 `@agentdash/ui/styles.css`。

### 为什么不是继续用 `app-web/styles.css`

`app-web/styles.css` 作为短期方案能修复问题，但工程语义不够好：桌面端需要从 Web app 包中拿主题，后续 `packages/views` 也会反向依赖具体 app 的视觉入口。既然不考虑旧技术债兼容，就不保留这条路径。标准 monorepo 形态应是：

```text
core   -> 无头逻辑
ui     -> 设计系统和基础组件
views  -> 业务视图
app-*  -> 宿主装配
```

这样 Tauri、Web、未来测试 harness 或 storybook 都只依赖 `@agentdash/ui`，不会依赖某个具体 app。

### Local Runtime 视图重构

最完整方案不是把 `@agentdash/views/local-runtime.css` 改成作用域 CSS，而是移除它：

- `@agentdash/views` 不再 export CSS。
- `LocalRuntimeView` 使用 `@agentdash/ui` primitives 与 Tailwind class 表达布局和状态。
- 通用控件沉淀为 `@agentdash/ui` primitives：
  - `Button`
  - `TextInput`
  - `Textarea`
  - `Select`
  - `Card`
  - `Badge`
  - `Field`
  - `EmptyState` / `MessageBox` 可按实际复用价值决定是否进入 UI 包。
- 仅业务结构留在 `LocalRuntimeView`，视觉规则来自 `@agentdash/ui`。

这样能避免“views 包自带 CSS”再次成为隐藏全局副作用。

### 桌面 shell 样式

`app-tauri/src/styles.css` 也不保留为全局壳样式入口。桌面 shell 直接使用 Tailwind class 和 `@agentdash/ui` primitives：

```tsx
<main className="grid min-h-screen grid-cols-[232px_minmax(0,1fr)] bg-background text-foreground">
  ...
</main>
```

如果桌面专属样式确实复杂，应优先抽成 `DesktopShell` 组件内部 class，而不是新增全局 CSS token。

### 桌面 Host 与 Dashboard

`app-tauri` 保持薄宿主：

```tsx
<DesktopShell>
  {activeView === "runtime" ? <LocalRuntimeView client={client} /> : <DashboardHost />}
</DesktopShell>
```

`DashboardHost` 继续：

1. 调 `desktop_api_snapshot()` 获取 API origin。
2. 请求 `${origin}/api/health`。
3. ready 后渲染 `WebDashboardApp`。

不可引入 Dashboard 组件复制，也不可让 Dashboard 直接调用 Tauri command 获取业务数据。

### 组件化演进路线

参考 `05-13-multica-local-runtime-concept-alignment`，整体方向是 `core/views/app` 分层，但本任务不一次性重构全部前端。

分阶段策略：

1. **设计系统先行**：新增 `@agentdash/ui`，把 Tailwind 主题、CSS token、base styles、共享组件 class 移出具体 app。
2. **样式一致性**：修复 Tailwind source、全局样式入口、CSS 变量污染，让 Tauri Dashboard 与 Web 对齐。
3. **宿主边界收束**：把桌面 shell、DashboardHost 的视觉表达改为 Tailwind / UI primitives，移除全局壳 CSS。
4. **共享视图沉淀**：将 Local Runtime 作为 `@agentdash/views` 的第一个成熟样板，确保它只依赖 `@agentdash/core` 端口和 `@agentdash/ui` 视觉系统。
5. **后续逐步迁移 Web feature**：只有当某个 Dashboard feature 真正需要跨 web/desktop 复用或测试隔离时，再从 `app-web/src/features` 拆到 `packages/views` / `packages/core`。

### Desktop-only Slot 方向

参考 multica 的 desktop settings slot 思路，AgentDash 后续可在 Web settings / runtime panel 中引入宿主扩展点：

- Web 宿主只展示 cloud/dashboard 通用设置。
- Desktop 宿主注入 Runtime、MCP、accessible roots、logs、版本/健康等本机能力 tab。
- 注入点通过 props/registry 实现，不 fork 整个 SettingsPage。

本任务只记录设计方向，不要求立即实现 slot 机制；实现范围以样式统一和边界收束为主。

## 数据流与边界

### Dashboard 数据流

```text
Tauri shell
  -> desktop_api_snapshot()
  -> GET {origin}/api/health
  -> WebDashboardApp
  -> app-web services/api
  -> agentdash-api HTTP/SSE
```

Dashboard 的事实源仍是 `agentdash-api`。

### Local Runtime 数据流

```text
LocalRuntimeView
  -> LocalRuntimeClient port
  -> app-tauri runtimeApi adapter
  -> Tauri invoke()
  -> agentdash-local-tauri commands
  -> LocalRuntimeManager / agentdash-local library
```

Local Runtime 视图不 import Tauri API。

## 重要取舍

- 不使用 iframe 承载 Web Dashboard。iframe 能隔离样式，但会引入路由、认证、SSE、窗口通信、焦点和菜单集成问题；本项目预研阶段应直接修正共享样式契约。
- 不让 `app-tauri` 自己 safelist 一大批 Tailwind class。safelist 只能治症状，不能解决跨 package source 边界和后续组件迁移问题。
- 不把所有 Web feature 立即搬到 `packages/views`。当前风险主要在样式契约和全局污染，一次性迁移会扩大范围。
- 不保留旧桌面 CSS 变量名。当前变量污染是根因之一，应直接改为命名空间 token。
- 不引入外部 UI 框架作为这次修复的基础。项目已有 Tailwind v4 + 自研组件规范，标准化重点应放在 monorepo 设计系统包，而不是换技术栈。
- 不保留 `app-web/styles.css` 或 `@agentdash/views/local-runtime.css` 作为兼容入口。预研阶段应直接删掉错误边界。

## 需要修改的高风险文件

- `packages/ui/src/styles.css`
  - 新设计系统样式事实源，会影响所有前端宿主。
- `packages/ui/package.json`
  - 新 workspace package，需要加入 exports 和 typecheck。
- `packages/app-web/src/styles/index.css`
  - 需要变成本 app 入口导入 `@agentdash/ui/styles.css`，不再承载样式事实源。
- `packages/app-web/src/main.tsx`
  - 若把外部 CSS 收敛进 `@agentdash/ui/styles.css`，入口 import 要同步精简。
- `packages/app-tauri/src/main.tsx`
  - 需要消费同一份 `@agentdash/ui` 样式契约，并确保 import 顺序不让桌面样式覆盖 Web 主题。
- `packages/app-tauri/src/App.tsx`
  - 桌面 shell 改为 Tailwind / UI primitives 可能影响截图和布局。
- `packages/app-tauri/src/styles.css`
  - 应删除或清空，不再作为全局样式入口。
- `packages/views/src/local-runtime/LocalRuntimeView.css`
  - 应删除，不再 export。
- `packages/views/src/local-runtime/LocalRuntimeView.tsx`
  - 需要改用 `@agentdash/ui` primitives 和 Tailwind class。
- `packages/app-web/package.json` / `packages/views/package.json`
  - 如新增明确样式 export 或调整 CSS 入口，需要同步 package export。
- `packages/app-tauri/package.json`
  - 需要新增 `@agentdash/ui` dependency，移除不再需要的样式间接依赖。
- 根 `package.json`
  - `shared:check` 需要纳入 `@agentdash/ui typecheck`。

## 验证策略

### 构建与类型

- `pnpm --filter app-web build`
- `pnpm --filter app-tauri build`
- `pnpm --filter @agentdash/ui typecheck`
- `pnpm run desktop:check`

### 样式产物检查

- 对比 `packages/app-web/dist/assets/index-*.css` 和 `packages/app-tauri/dist/assets/index-*.css` 的规模与关键 utility。
- 检查 `app-tauri` 产物中存在 Dashboard 关键 utility，例如 `rounded-[8px]`、`bg-primary/10`、`text-muted-foreground`、`hover:bg-secondary`、响应式 grid class。
- 检查产物中不再出现 Local Runtime 对 `:root --border: #...` 这类污染。

### 视觉检查

- 启动 `pnpm dev`。
- 对比浏览器 Web Dashboard 与 Tauri Dashboard 首屏。
- 检查：
  - sidebar 宽度和背景。
  - project card。
  - agent card。
  - session list row。
  - button、input、badge、status pill。
  - Markdown / KaTeX 样式不缺失。

### 行为检查

- DashboardHost 在 API starting/error/running 时展示正确状态。
- Runtime tab 能启动、停止、重启、保存 profile、展示 logs、管理 MCP servers。
- Dashboard tab 不直接调用 Tauri command 获取 Dashboard 业务数据。

## 后续任务候选

- `refactor(frontend): Dashboard 可复用 views 分层`
- `refactor(frontend): server state query layer 规范化`
- `feat(desktop): Settings desktop-only slot 注入`
- `feat(desktop): local runtime health/log UI 体验增强`
- `feat(desktop): cloud relay status 与 local command status 融合展示`
