# Tauri 桌面端与 Web 前端样式和组件边界统一规划

## Goal

规划并实施一条长期正确的桌面端前端架构路线，使 Tauri 本地运行 app 中的 Web Dashboard 与浏览器 Web 前端保持同一套视觉系统、组件边界和业务视图来源。

本任务要解决当前 Tauri Dashboard 与 Web Dashboard 视觉差异巨大的问题，并把它从一次样式补丁提升为前端工程化边界收束：新增 `@agentdash/ui` 作为设计系统包，统一管理 Tailwind 主题、CSS token、基础 UI class / primitives 和共享样式入口；`app-web` 负责 Web 宿主，`app-tauri` 负责桌面宿主与 Tauri command 适配，`@agentdash/views` 承载可跨宿主复用的业务视图，`@agentdash/core` 承载无头逻辑和类型化端口。

任务仍处规划阶段。实现前需要先确认本 PRD、`design.md` 和 `implement.md`。

本任务采用“最妥善的完整解决方案”作为目标，不以现有技术债或临时兼容为约束。允许直接移除旧的样式事实源和全局 CSS 副作用，把前端视觉系统收束到长期最正确的架构。

## Confirmed Facts

- 当前仓库已有 `packages/app-web`、`packages/app-tauri`、`packages/core`、`packages/views` 四个前端相关 workspace package。
- `packages/app-tauri/src/App.tsx` 通过 `import WebDashboardApp from 'app-web'` 复用 Web Dashboard，而不是复制 Web Dashboard 组件树，这符合 `.trellis/spec/cross-layer/desktop-local-runtime.md` 的现有契约。
- `packages/app-tauri/src/main.tsx` 导入 `app-web/styles.css`、`./styles.css`、`@agentdash/views/local-runtime.css`；`packages/app-web/src/main.tsx` 额外导入 `streamdown/styles.css` 和 `katex/dist/katex.min.css`。两端入口的全局样式集合不一致。
- `packages/app-web/src/styles/index.css` 使用 Tailwind CSS v4，并通过 HSL CSS 变量定义 `--border`、`--primary`、`--muted` 等主题 token。
- `packages/views/src/local-runtime/LocalRuntimeView.css` 在 `:root` 上定义了 `--border: #d8dee8`、`--primary: #2563eb`、`--muted: #647084` 等同名变量，覆盖 Web 主题变量后会破坏 `hsl(var(--border))` / `hsl(var(--primary))` 等 Tailwind 生成规则。
- 构建对比显示 `app-web` 主 CSS 约 109KB，`app-tauri` 主 CSS 约 38KB。桌面端 CSS 产物缺少大量 Web Dashboard 使用的 Tailwind utility，是当前视觉崩坏的主要证据之一。
- 参考任务 `.trellis/tasks/05-13-multica-local-runtime-concept-alignment` 的桌面端结论建议学习 multica 的 `core/views/ui/app` 分层、desktop-only slot 注入、IPC bridge 隔离本机能力、desktop 作为本机能力控制台，而不是把 desktop 当作简单 Web wrapper。
- 用户倾向采用更标准的工程化方案一步到位，因此本任务不再以 `app-web` 临时代管全局样式为目标，而是规划新增 `packages/ui` / `@agentdash/ui` 设计系统包作为样式事实源。
- 用户明确不希望为了既有技术债保留兼容式过渡；本任务应选择最合适的终态方案，而不是最小改动方案。
- 本项目处于预研期，不需要兼容旧入口或保留回退方案；应直接整理到长期正确状态。

## Requirements

- 统一 Web 与 Tauri 的视觉入口：
  - 新增 `@agentdash/ui` 设计系统包，作为 Tailwind 主题、CSS token、base layer、共享 component class 和后续 UI primitives 的事实源。
  - `@agentdash/ui/styles.css` 必须成为 Web Dashboard 与 Tauri Dashboard 的唯一全局 CSS 入口，包含 Tailwind 主题、Tailwind content/source 扫描范围、Markdown/KaTeX 等 Dashboard 必需的全局样式。
  - `app-web` 浏览器入口和 `app-tauri` Dashboard 入口必须消费同一份 `@agentdash/ui` 样式契约。
  - 移除 `app-web/styles.css` 作为跨包样式依赖入口；`app-tauri` 不再从 `app-web` 获取样式。
  - `app-tauri` 不能通过额外全局 CSS 覆盖 Web Dashboard 的主题变量。
- 修复桌面本机 runtime UI 的样式边界：
  - 移除 `@agentdash/views/local-runtime.css` 这个 package-level 全局 CSS export；`@agentdash/views` 不应携带全局样式副作用。
  - Local Runtime 视图改为使用 `@agentdash/ui` primitives / Tailwind class。
  - 桌面壳样式改为 Tailwind class 或 `@agentdash/ui` primitives，不保留独立全局 `app-tauri/src/styles.css` 作为样式事实源。
  - 不再占用或覆盖 `--border`、`--primary`、`--muted` 等 Web 主题 token。
- 明确前端包边界：
  - `packages/ui` / `@agentdash/ui`：设计系统事实源，承载 Tailwind v4 CSS 入口、主题 token、base styles、基础 UI primitives、共享 UI class。
  - `packages/app-web`：浏览器 Web 宿主、路由、认证、Web Dashboard 装配入口。
  - `packages/app-tauri`：Tauri 桌面宿主、桌面导航壳、Tauri command adapter、DashboardHost 健康检查。
  - `packages/views`：无宿主依赖的可复用业务视图，优先承载 Local Runtime 这类跨宿主 UI，后续逐步承载 Dashboard 可复用视图。
  - `packages/core`：无 React 宿主依赖的类型、端口、纯函数、client contract。
- 桌面 Dashboard 不得绕过 Web Dashboard 数据 authority：
  - Dashboard 仍通过 HTTP API 访问 `agentdash-api`。
  - Local Runtime 管理面板才通过 `LocalRuntimeClient` 端口访问 Tauri command。
- 规划要评估整体组件化方向：
  - 先建立 `@agentdash/ui`，把视觉事实源从具体 app 中抽离。
  - 再解决视觉一致性和样式污染。
  - 同步把 Local Runtime 这条 desktop 关键视图整理为 `@agentdash/views` + `@agentdash/ui` 的样板。
  - 不为了兼容旧 CSS 入口保留 wrapper 或双入口。
  - 不一次性重构全部 Web feature 到 `packages/views`；这不是技术债兼容，而是任务边界控制。只有当前样式和 desktop 关键路径涉及的视图进入本任务。
  - 后续可按 feature 逐步迁移到 `core/views/ui/app` 分层。
- 验证必须覆盖桌面与 Web 双入口：
  - 构建检查、类型检查、桌面检查。
  - 至少对 Dashboard 首屏在 Web 与 Tauri/desktop renderer 的关键样式做人工或自动截图对比。
  - 检查主 CSS 产物不再出现桌面端缺失大量 Web utility 的情况。

## Acceptance Criteria

- [ ] `app-tauri` 中 Web Dashboard 首屏与 `app-web` 浏览器首屏在布局、间距、圆角、按钮、输入框、状态标签、列表行高、颜色 token 上保持一致。
- [ ] `@agentdash/views/local-runtime.css` export 被移除，`LocalRuntimeView` 不依赖 package-level 全局 CSS。
- [ ] 新增 `packages/ui` / `@agentdash/ui`，并明确作为 Tailwind 主题、CSS token、base styles 和共享样式入口的事实源。
- [ ] `app-web` 与 `app-tauri` 复用同一份 `@agentdash/ui` 样式契约；不存在 `app-web/styles.css` 作为跨包样式入口。
- [ ] `app-tauri/src/styles.css` 不再作为全局桌面壳样式入口；桌面壳使用 Tailwind class / `@agentdash/ui` primitives。
- [ ] Tailwind v4 的 content/source 范围显式覆盖 `app-web`、`app-tauri`、`@agentdash/views` 中需要生成 utility 的 TS/TSX 源码。
- [ ] `streamdown/styles.css`、`katex/dist/katex.min.css` 等 Web Dashboard 必需全局样式在 Web 和 Tauri Dashboard 中一致加载。
- [ ] `app-tauri` 不复制 `packages/app-web/src` 下的 Dashboard 组件树；只通过公开 package export 复用 Web Dashboard。
- [ ] `packages/core` / `packages/views` / `packages/app-web` / `packages/app-tauri` 的职责边界在 `design.md` 中说明清楚，并与 `.trellis/spec/cross-layer/desktop-local-runtime.md` 保持一致。
- [ ] `implement.md` 给出可分阶段执行的实现清单、验证命令、风险文件和回滚点。
- [ ] 规划文档参考并吸收 `05-13-multica-local-runtime-concept-alignment` 中关于 `core/views/ui/app` 分层、desktop-only slot、IPC bridge 和本机能力控制台的结论。
- [ ] 完成后通过 `pnpm --filter @agentdash/ui typecheck`、`pnpm --filter app-web build`、`pnpm --filter app-tauri build`、`pnpm run desktop:check`。

## Notes

- 本任务是复杂任务，必须保留 `design.md` 和 `implement.md`。
- 本任务不追求旧桌面样式兼容；当前目标是直接修到正确架构，不保留兼容 wrapper 或双事实源。
- 本任务不改变后端 API、数据库字段或迁移。
- 相关参考：
  - `.trellis/spec/frontend/component-guidelines.md`
  - `.trellis/spec/frontend/directory-structure.md`
  - `.trellis/spec/cross-layer/desktop-local-runtime.md`
  - `.trellis/tasks/05-13-multica-local-runtime-concept-alignment/research/desktop-local-integration.md`
