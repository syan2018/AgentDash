# Implement — 桌面外壳设计重塑与错误体系

承接 `design.md`。按阶段顺序执行；每阶段末有校验与 review gate，可独立 commit / 回滚。

## 阶段 P0 — 勘定（动手前）

- [ ] P0.1 确认 Tauri capabilities 源文件位置（`crates/agentdash-local-tauri/capabilities/*.json` 或 `tauri.conf.json` 内引用），记录现有 window 权限。
- [ ] P0.2 grep 清点 `variant="primary"` 与 `agentdash-button-primary` 使用点，列出"实心化后可能过吵"的页面，决定是否有误用需顺手改 secondary。
- [ ] P0.3 确认 app-web 根容器高度来源（`index.html` / `main.tsx`），为 titlebar 预留高度做准备。

## 阶段 P1 — 设计基座（packages/ui）【R1 + R2.2】

- [ ] P1.1 `styles.css`：新增 `--shadow-sm/md/lg`（light + dark 两套），在 `@theme inline` 暴露为可用的 shadow utility。
- [ ] P1.2 `Button.tsx` + `agentdash-button-primary`：primary 改实心填充；其余 variant 不动。
- [ ] P1.3 新增 sidebar surface 变量（若用独立 tint 而非直接复用 card）。
- [ ] P1.4 新增 `StatusScreen` primitive（`packages/ui/src/primitives/StatusScreen.tsx`）+ `index.ts` 导出；props：`tone`(loading/info/warning/danger) / `title` / `description` / `action?` / `brand?`。
- [ ] P1.5 `DesignSystemPage` 补 elevation 预览、sidebar-vs-content 对比、Button 新形态、StatusScreen 预览。
- **校验**：`pnpm run shared:check`；`pnpm --filter app-web run typecheck && pnpm --filter app-web lint`（0 warning）；本地开 `/dev/design-system` 目检 light/dark。
- **Review gate G1**：在 `/dev/design-system` 确认纵深/按钮/表面观感达标后再继续。

## 阶段 P2 — 错误体系（app-web）【R2】

- [ ] P2.1 新增 `AppErrorBoundary`（class 组件）→ fallback 用 `StatusScreen` danger 态 + "重载应用"。
- [ ] P2.2 `App.tsx`：顶层用 `AppErrorBoundary` 包 `AuthGate`/`AppContent`；`RouteFallback`→StatusScreen loading；`BootstrapErrorState`→StatusScreen danger。
- [ ] P2.3 `WorkspaceLayout`：`<Outlet>` 外包路由级 boundary（`resetKeys={[location.pathname]}`）。
- [ ] P2.4 临时注入一个会 throw 的测试组件，手动验证崩溃屏出现、重载/路由切换可恢复，验证后移除（或留作 dev-only）。
- **校验**：`pnpm --filter app-web run typecheck && lint`；手动触发崩溃验证不白屏。
- **Review gate G2**：错误屏视觉与可恢复性确认。

## 阶段 P3 — sidebar 表面落地（app-web）【R1.2】

- [ ] P3.1 `workspace-layout.tsx`：sidebar 改独立表面 + 阴影收边（替代/弱化 `border-r`）。
- [ ] P3.2 回归 sidebar 内子元素 hover/active 对比：ProjectDropdown、NavLink active 态、SessionShortcutList、SidebarFooter，逐个目检 light/dark。
- **校验**：typecheck + lint + 目检双模式。

## 阶段 P4 — Tauri 外壳 + splash【R3】

- [ ] P4.1 `tauri.conf.json`：`app.windows[0].decorations = false`。
- [ ] P4.2 新增 `DesktopTitlebar`（app-tauri 本地）：drag region + min/max/close（`@tauri-apps/api/window`），仅 `__TAURI_INTERNALS__` 存在时渲染。
- [ ] P4.3 app-tauri `App.tsx`：`flex-col` 容器 = titlebar + 下方应用区；splash（`DashboardHost`）渲染分支改用 `StatusScreen`，保留 `/api/health` gate 状态机。
- [ ] P4.4 capabilities：补 `core:window:allow-minimize / allow-toggle-maximize / allow-close / allow-start-dragging`。
- [ ] P4.5 app-web 高度适配：根容器在 Tauri 下填充父级（不强制 `h-screen`），Web 形态仍全屏。
- [ ] P4.6 窗口效果：保持**实色 titlebar/表面**为基线（不引入 window-vibrancy）；effect 列为后续 stretch。
- **校验**：`pnpm run desktop:check`（shared + app-tauri typecheck + `cargo check -p agentdash-local-tauri`）；`pnpm run dev:desktop` 实机验证拖拽/最小化/最大化/关闭、splash 三态、Web 形态无 titlebar。
- **Review gate G3**：实机外壳确认。

## 阶段 P5 — 规范与验收【R1.4 / C4 / AC6】

- [ ] P5.1 更新 `.trellis/spec/frontend/design-language.md`：shadow token（§2）、surface 阴影规则与修订后的"嵌套二选一"（§4）、Button 实心描述（§6）。
- [ ] P5.2 Playwright 烟测：splash→ready、错误屏出现（参考 `tests/e2e/`，`playwright.config.ts`）。
- [ ] P5.3 全量校验。

## 最终校验命令

```bash
pnpm run shared:check
pnpm --filter app-web run typecheck
pnpm --filter app-web lint          # 期望 0 warning（C4）
pnpm --filter app-web test
pnpm run desktop:check              # 含 cargo check -p agentdash-local-tauri
pnpm run dev:desktop                # 实机：titlebar / splch / 双形态
pnpm exec playwright test <新增烟测>
```

## 回滚点

- P1 后：revert `packages/ui` + design-language + DesignSystemPage → 业务页回旧观感。
- P2 后：移除 boundary 包裹 → 回到无兜底。
- P4 后：`decorations` 改回默认 + 还原 app-tauri/App.tsx → 回系统标题栏 + 旧 splash 逻辑。

## 备注

- 每阶段独立 commit，遵循仓库 commit 规范（task.py 的 auto-commit / 项目 git 习惯）。
- ESLint `no-restricted-syntax` 是 warn 级，触达 UI 必须收敛到 token/radius/surface 规则（C4）。
