# 实施计划：Tauri 桌面端与 Web 前端样式和组件边界统一

## 阶段 0：基线确认

1. 记录当前 Web 与 Tauri 样式差异。
   - 构建 `app-web` 与 `app-tauri`。
   - 记录主 CSS 文件大小。
   - 搜索桌面 CSS 产物中是否缺少 Dashboard 常用 utility。
2. 确认当前污染来源。
   - 搜索 `--border`、`--primary`、`--muted`。
   - 确认 `@agentdash/views/local-runtime.css` 的 `:root` 变量覆盖。
3. 确认入口差异。
   - 对比 `packages/app-web/src/main.tsx` 与 `packages/app-tauri/src/main.tsx` 的 CSS import。

## 阶段 1：新增 `@agentdash/ui` 设计系统包

1. 创建 `packages/ui`。
   - `package.json` 使用 `name: "@agentdash/ui"`。
   - 添加 `typecheck` 脚本。
   - 添加 exports，例如：
     - `"."`: primitives 入口。
     - `"./styles.css"`: 设计系统 CSS 入口。
     - `"./tokens.css"`: 可选 token-only 入口。
2. 创建最小但真实可用的 primitives。
   - `Button`
   - `TextInput`
   - `Textarea`
   - `Select`
   - `Card`
   - `Badge`
   - `Field`
   - 如需要 class 合并工具，新增 `cn`，并在 `@agentdash/ui` 中引入 `clsx` / `tailwind-merge` 或等价实现。
3. 创建 `packages/ui/src/styles.css`。
   - 保留 Tailwind v4 主题和现有组件层。
   - 显式添加 Tailwind source 范围，覆盖 `app-web`、`app-tauri`、`packages/views` 中会被共同构建的 TS/TSX。
   - 保留第三方 streamdown 相关 source。
4. 把 Web Dashboard 必需全局样式收敛到 `@agentdash/ui/styles.css`。
   - 将 `streamdown/styles.css` 与 `katex/dist/katex.min.css` 纳入统一入口。
   - `app-web` 和 `app-tauri` 入口只维护同一个设计系统 style import 清单。
5. 调整 package dependencies。
   - `app-web`、`app-tauri`、`@agentdash/views` 显式依赖 `@agentdash/ui`。
   - 移除 `app-web` 中 `./styles.css` export，不保留跨包样式兼容入口。
   - 移除 `@agentdash/views/local-runtime.css` export。
6. 更新根脚本。
   - `shared:check` 纳入 `pnpm --filter @agentdash/ui typecheck`。

## 阶段 1.5：统一宿主样式消费

1. 修改 `packages/app-web/src/styles/index.css`。
   - 改为导入 `@agentdash/ui/styles.css`。
   - 移除已迁移到 `@agentdash/ui` 的主题、base、component 重复定义。
2. 修改 `packages/app-web/src/main.tsx`。
   - 移除重复的第三方 CSS import，避免 Web 与 Tauri 维护两份清单。
3. 修改 `packages/app-tauri/src/main.tsx`。
   - 直接导入 `@agentdash/ui/styles.css`。
   - 移除 `app-web/styles.css` 和 `@agentdash/views/local-runtime.css` import。
   - 移除 `./styles.css` import，桌面 shell 改为组件内 Tailwind class。

## 阶段 2：作用域化 Local Runtime 与桌面 shell 样式

1. 修改 `packages/views/src/local-runtime/LocalRuntimeView.tsx`。
   - 使用 `@agentdash/ui` primitives 替代 plain CSS class 控件。
   - 使用 Tailwind class 表达布局、grid、spacing、状态色。
   - 移除对 `panel`、`field`、`actions`、`status-pill` 等宽泛 class 的依赖。
2. 修改 `packages/views/src/local-runtime/LocalRuntimeView.css`。
   - 删除该文件。
   - 删除 `packages/views/package.json` 中的 CSS export。
3. 修改 `packages/app-tauri/src/App.tsx` 与桌面 shell 样式。
   - 使用 Tailwind class / `@agentdash/ui` primitives 表达桌面 shell。
   - 移除 `desktop-shell` / `sidebar` / `nav-item` 等全局 class。
   - DashboardHost 状态页使用 UI primitives 和 Tailwind class。
4. 删除 `packages/app-tauri/src/styles.css` 或清空后移除 import。
5. 保证 CSS import 顺序。
   - `@agentdash/ui` 主题先加载。
   - 不再存在桌面和 Local Runtime 全局 CSS 覆盖 Web Dashboard 通用主题 token。

## 阶段 3：边界收束与小型组件化

1. 保持 `app-tauri` 通过 `app-web` package export 复用 `WebDashboardApp`。
2. 确认 `LocalRuntimeView` 只依赖 `LocalRuntimeClient` port，不 import Tauri API。
3. 如桌面 shell 逻辑变大，将其拆到 `packages/app-tauri/src/DesktopShell.tsx` 和 `DashboardHost.tsx`。
4. 不在本任务中迁移全部 `app-web/src/features` 到 `packages/views`。
5. 在任务总结中记录后续 `core/views/app` 分层候选：
   - `LocalRuntimeView` 作为当前样板。
   - `@agentdash/ui` 作为设计系统入口。
   - Settings desktop slot。
   - Dashboard 业务 feature 的渐进迁移。

## 阶段 4：验证

1. 运行构建与检查。
   - `pnpm --filter app-web build`
   - `pnpm --filter app-tauri build`
   - `pnpm --filter @agentdash/ui typecheck`
   - `pnpm run desktop:check`
2. 检查 CSS 产物。
   - 桌面主 CSS 不应只有 Web 主 CSS 的很小子集。
   - 桌面产物应包含 Dashboard 首屏所需 utility。
   - 不应存在 Local Runtime 对 Web 主题 token 的全局覆盖。
3. 启动本地开发。
   - `pnpm dev`
   - 按项目说明，Rust 后端和 Tauri 壳不能热重载，改 Rust 或 Tauri 后需杀进程重启。
4. 做视觉比对。
   - Web Dashboard 与 Tauri Dashboard 首屏对比。
   - 重点看项目卡片、Agent cards、搜索框、会话列表、状态标签、按钮、输入框。
5. 做 Local Runtime tab smoke。
   - Runtime snapshot。
   - profile load/save/delete。
   - start/stop/restart。
   - logs tail/clear/copy。
   - MCP server load/save/probe。

## 风险与回滚点

- `packages/ui/src/styles.css`
  - 风险：Tailwind source 范围过宽可能增加 CSS 产物。
  - 回滚：恢复 source 范围并改为更精确路径。
- `packages/app-web/src/styles/index.css`
  - 风险：迁移时漏掉现有 component class。
  - 回滚：对照迁移前 CSS 补齐到 `@agentdash/ui/styles.css`，不要恢复多事实源。
- `packages/views/src/local-runtime/LocalRuntimeView.css`
  - 风险：删除 CSS 后 Local Runtime 面板局部样式丢失。
  - 回滚：通过截图和 class 搜索补齐 Tailwind / UI primitive，不恢复 CSS 文件。
- `packages/app-tauri/src/main.tsx`
  - 风险：移除旧 import 后桌面 shell 样式遗漏。
  - 回滚：补齐组件内 Tailwind class，不恢复旧全局 CSS。
- `packages/app-web/src/main.tsx`
  - 风险：外部 CSS 收敛后 Web 入口漏样式。
  - 回滚：检查 shared style export 是否包含第三方 CSS，而不是让 Web/Tauri 分别手写 import。

## 验收前检查清单

- [ ] 没有新增 Dashboard 组件复制。
- [ ] 没有 iframe 方案。
- [ ] 没有 Tailwind 大规模 safelist 作为主要方案。
- [ ] 没有 Tauri command 直接进入 Dashboard 业务数据流。
- [ ] 没有 `:root --border: #...` 这类覆盖 Web 主题 token 的代码。
- [ ] `@agentdash/ui` 是唯一设计系统事实源。
- [ ] 不保留 `app-web/styles.css` 作为跨包样式入口。
- [ ] 不保留 `@agentdash/views/local-runtime.css` CSS export。
- [ ] 不保留 `app-tauri/src/styles.css` 全局壳样式入口。
- [ ] Web 与 Tauri 的 CSS 入口说明清楚，且均消费 `@agentdash/ui`。
- [ ] 后续组件化拆分候选已记录，但未扩大到本任务之外。
