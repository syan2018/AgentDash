# 支持用户退出登录

## Goal

让已登录用户能从主界面主动退出当前账号：清除前端登录态、撤销服务端 JWT、回到登录页。

当前缺口（详见 [research/auth-current-state.md](research/auth-current-state.md)）：

- 后端 `POST /api/auth/logout` 已就绪（`crates/agentdash-api/src/routes/auth_routes.rs:147-163`）。
- 前端 `useAuthStore.logout()` 存在，但**没有 UI 入口**、**不调后端**、**不断流**。
- 结果：用户没法退出登录；即便 dev 手工清 token，服务端会话也仍然有效到 JWT 自然过期。

## User Value

- 共用机器场景下用户能干净地退出（之前只能清浏览器存储）。
- 服务端 token 在退出时被立即撤销，安全语义闭环。
- UserCard 由"纯展示"升级为"用户菜单"，留出后续放"个人中心 / 切换账号 / 偏好"的扩展位。

## Confirmed Facts

来自 [research/auth-current-state.md](research/auth-current-state.md)，本 PRD 的执行边界基于以下事实：

- 后端 logout 端点：`POST /api/auth/logout` → `revoke_token` → SQL 标记 `revoked_at` → 204。**前端不需要扩后端**。
- AuthGate（`packages/app-web/src/App.tsx:163-243`）在未登录时直接 render `<LoginPage />`，**logout 不需要 `navigate`**，靠状态变化重渲染即可。
- BYOK 凭据存在服务端 `llm_provider` 表里，与登录 token 完全解耦。**logout 不动 BYOK**。
- 桌面端 local runtime 是机器绑定的常驻进程，access_token 只是它对外发请求时挂的凭据。logout 撤销后端 token 后，runtime 嵌的那份 token 自动失效，**logout 不需要 stop runtime**。
- 现有 `UserCard`（`packages/app-web/src/components/layout/workspace-layout.tsx:915-945`）是底栏纯展示组件，本任务把它改造成 popover trigger。
- 唯一的 HTTP 客户端是 `packages/app-web/src/api/client.ts`，token 读写入口 `setStoredToken / getStoredToken / clearStoredToken` 已覆盖 localStorage + cookie 双写。
- 没有全局 401 拦截器，但 AuthGate 在 `/api/me` 收到 401 时会 `clearStoredToken()`——这意味着 logout 后即便有遗漏的 in-flight 请求 401，也不会出现"已登出但 UI 还没反应"的边角案例。

## Requirements

### R1 · 前端 logout API 封装

- 在 `packages/app-web/src/api/auth.ts` 新增 `postLogout()`：调用 `POST /api/auth/logout`，返回值忽略（204）。
- 走现有 `api`/`authenticatedFetch` 体系，自动注入当前 Bearer token。

### R2 · `useAuthStore.logout()` 扩展

把 `packages/app-web/src/stores/authStore.ts` 的 `logout()` 从"只清本地状态"扩展为完整退出流程，**顺序不变**：

1. **Fire-and-forget** 调用 `postLogout()`（不 await，失败仅 `console.warn`）——服务端 token 撤销不阻塞 UI。
2. `closeAllStreamConnections()`（`packages/app-web/src/api/streamRegistry.ts`）—— 关掉 session NDJSON 流。
3. `useEventStore.getState().disconnect()` —— 关掉项目事件流。
4. `useCurrentUserStore.getState().clear()`（已有）。
5. `clearStoredToken()`（已有，清 localStorage + cookie）。
6. `set({ loginError: null })`（已有）。

**显式不做**：

- 不停桌面 local runtime（见 Confirmed Facts）。
- 不清 BYOK store / 不调 BYOK 接口。
- 不主动 navigate（AuthGate 自动 render LoginPage）。
- 不显示 toast / 不显示"已登出"页面。

### R3 · UserCard → Popover 改造

把 `UserCard`（`packages/app-web/src/components/layout/workspace-layout.tsx:915-945`）从静态 div 改为 popover trigger：

- **Trigger**（外观保持现有 UserCard 形态）：头像 + display_name + email + Admin/auth_mode badge。增加 hover/focus 视觉反馈，鼠标变手型。
- **Popover 内容**：
  - 顶部身份块：更大头像、display_name、email、provider 名（取自 `currentUser.provider`）、auth_mode 文本（"个人 / 企业"）、Admin 徽标（如有）。
  - 分割线。
  - "退出登录" 行（红色调或 destructive 风格），点击直接调 `useAuthStore.getState().logout()`。**不二次确认**。
- 复用 `packages/ui` 已有的 popover primitive（参考 BackendPanel / SidebarFooter 现有 popover 模式，保证视觉一致）。

### R4 · 不引入新依赖、不动后端

- 不新增 npm 包 / Rust crate。
- 不改 `crates/agentdash-api`、`crates/agentdash-application` 等后端代码。
- 不动 BYOK 相关文件（`packages/app-web/src/features/settings/ui/UserByokSection.tsx`、`stores/llmProviderStore.ts` 等）。
- 不动 Tauri 桌面端 runtime 启停（`packages/app-web/src/desktop/localRuntimeBridge.ts` 不改）。

## Acceptance Criteria

### 行为

- [ ] **AC1** 已登录用户在侧边栏底部点击 UserCard，弹出 popover，能看到自己的头像、display_name、email、auth_mode、（如是 admin）Admin 徽标、provider 名。
- [ ] **AC2** 点击 popover 中的"退出登录"，UI 立即返回登录页（无 confirm dialog、无 toast、无可见过渡停留 > 200ms）。
- [ ] **AC3** 退出后 `localStorage.agentdash_access_token` 与同名 cookie 都为空；`useCurrentUserStore.currentUser` 为 null。
- [ ] **AC4** 退出后 1 秒内服务端 `auth_session.revoked_at` 被标记（可通过观察前端 Network 看到 `POST /api/auth/logout` 触发；服务端核查为可选，若便利可在 SQL 中观察）。
- [ ] **AC5** 用刚撤销的 token 直接重放任意 secured API（如 `GET /api/me`），返回 401。
- [ ] **AC6** 退出过程中如果后端 `POST /api/auth/logout` 失败（断网 / 5xx），UI 仍然立即返回登录页；`console.warn` 有一条日志，但不弹错误。

### 隔离

- [ ] **AC7** 退出登录**不**清除 BYOK 凭据：登出再登入同一账号后，Settings → UserByokSection 看到的凭据列表不变。
- [ ] **AC8** 退出登录**不**停止桌面端 local runtime；用户重新登录后，runtime 自动以新 token 继续工作（无需手工重启）。

### 非回归

- [ ] **AC9** 表单登录 / OIDC redirect 登录两条路径仍正常工作。
- [ ] **AC10** AuthGate 收到 `/api/me` 401 时仍能自动回退到登录页（原行为）。

## Out of Scope

- 切换账号（一键 logout-then-login 同入口）。
- 多设备会话管理 UI（"我的活跃会话"列表）。
- 二次确认 dialog。
- Logout toast / 过渡动画 / "已退出" 中间页。
- 桌面端 runtime stop / pause。
- 自动清理过期 `auth_session` 记录的 cron job。
- BYOK 凭据撤销 / 切换账号时的 BYOK 行为讨论。
- 业务 API 全局 401 拦截器（与本任务正交，留作后续独立任务）。

## Open Questions

- 无（待用户 review 后确认）。

## Notes

任务等级：**lightweight**——纯前端改动，预计涉及 3 个文件：

- `packages/app-web/src/api/auth.ts`（新增 `postLogout`）
- `packages/app-web/src/stores/authStore.ts`（扩展 `logout`）
- `packages/app-web/src/components/layout/workspace-layout.tsx`（UserCard popover 改造，可能拆出 `UserMenuPopover` 子组件）

按 Trellis 规则 lightweight 任务可 PRD-only 进入实现；不强制 design.md / implement.md。
