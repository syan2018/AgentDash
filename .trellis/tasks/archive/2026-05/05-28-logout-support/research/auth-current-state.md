# Research: 当前登录 / 认证体系

- **Query**: 在写 logout PRD 之前，把当前登录/认证全链路的资产盘清楚
- **Scope**: 内部代码（packages/app-web 前端 + crates/agentdash-* 后端）
- **Date**: 2026-05-28

## 概述

AgentDash 是一个 monorepo（pnpm workspaces）：

- 前端：`packages/app-web`（React 19 + Zustand 5 + react-router-dom 7 + Vite 7）
- 后端：`crates/agentdash-api`（Rust + axum）+ `agentdash-application` / `agentdash-domain` / `agentdash-infrastructure`（PostgreSQL）
- 桌面壳：`packages/app-tauri`（Tauri）+ `crates/agentdash-local-tauri`
- UI 库：`packages/ui`（shadcn-ish primitives）

登录走 Bearer JWT；后端持有 `auth_session` 表用于按 token hash 反查身份并支持撤销。已经存在完整的后端 logout/revoke 端点，**前端只有 `useAuthStore.logout()` 函数本地清状态，但没有任何 UI 入口能调用它，也没有调用后端 `/api/auth/logout`**。

---

## 1. Login entry points（登录入口）

| 文件 | 说明 |
|---|---|
| `packages/app-web/src/pages/LoginPage.tsx` | 唯一登录页面组件（无路由 path——通过 `AuthGate` 在未登录时直接 render） |
| `packages/app-web/src/App.tsx:163-243` | `AuthGate`：未登录时 render `<LoginPage />`，已登录时放行 children |
| `packages/app-web/src/stores/authStore.ts` | Zustand store，持有 `metadata / login / startRedirectLogin / logout` |
| `packages/app-web/src/api/auth.ts` | `fetchLoginMetadata` / `postLogin` / `startRedirectLogin` 三个 API 封装 |

登录页支持两种模式（`metadata.login_mode`）：

- `"form"`：从 `/api/auth/metadata` 拉 `LoginFieldDescriptor[]`，渲染用户名/密码（或自定义字段）表单，提交到 `/api/auth/login`（`packages/app-web/src/pages/LoginPage.tsx:65-79`）。
- `"redirect"`：调用 `POST /api/auth/oidc/start` 拿 `auth_url` 后 `window.location.assign(auth_url)`（`packages/app-web/src/stores/authStore.ts:58-71`）。OIDC callback 由后端 `GET /api/auth/oidc/callback` 处理，set-cookie 后 302 跳回前端。

设置页里还有一个**子流程**："个人 Codex BYOK 用 ChatGPT OAuth 登录"，与主登录是**两个独立的概念**——见第 4 节。

---

## 2. Auth state storage（登录态存储）

### 前端

| 位置 | 内容 | Key |
|---|---|---|
| `localStorage` | JWT access token | `agentdash_access_token` （`packages/app-web/src/api/client.ts:3`） |
| `cookie` | 同一 JWT（兜底，主要供 OIDC callback set-cookie 后第一次刷新读取）| `agentdash_access_token`，`Path=/; Max-Age=30天; SameSite=Lax`（`packages/app-web/src/api/client.ts:4-5, 107-119`） |
| Zustand `useAuthStore` | `metadata / isLoginLoading / loginError`（**不持有 token**）| `packages/app-web/src/stores/authStore.ts:7-18` |
| Zustand `useCurrentUserStore` | `currentUser / isLoading / hasLoaded / error`，由 `/api/me` 填充 | `packages/app-web/src/stores/currentUserStore.ts` |

读取顺序：`getStoredToken()` 先读 localStorage，没有再读 cookie（`packages/app-web/src/api/client.ts:7-13`）。

### 后端

`auth_session` 表（PostgreSQL，见 `crates/agentdash-infrastructure/migrations/0001_init.sql` 与 `crates/agentdash-infrastructure/src/persistence/postgres/auth_session_repository.rs`）：

- 列：`token_hash` (sha256(token)) / `identity_json` / `expires_at` / `revoked_at` / `created_at` / `updated_at`
- 通过 `AuthSessionService::save_login_session` 写入（`crates/agentdash-application/src/auth/session_service.rs:36-55`）
- 通过 `AuthSessionService::resolve_identity_by_token` 在认证 provider 失败时回源（`crates/agentdash-api/src/auth.rs:120-156`）
- 通过 `AuthSessionService::revoke_token` 标记 `revoked_at`（`crates/agentdash-application/src/auth/session_service.rs:85-91`）

后端没有传统的"server-side session cookie"——cookie 里装的就是和 localStorage 同一份 JWT。

---

## 3. Credentials & tokens（凭证）

- **类型**：JWT（access token，可以解析 `exp` claim——见 `crates/agentdash-application/src/auth/session_service.rs:115-128`）。
- **没有 refresh token**：代码中无 `refresh_token` 字段、无刷新流程。token 过期后请求会 401，前端 `AuthGate` 在 `/api/me` 收到 401 时执行 `clearStoredToken()` 并跳回登录页（`packages/app-web/src/App.tsx:189-194, 219-224`）。
- **传输方式**：
  - 主路径：HTTP `Authorization: Bearer <token>`，由 `request()` 在 `packages/app-web/src/api/client.ts:35-38` 注入；非 `api.*` 调用使用 `authenticatedFetch()` 注入（`packages/app-web/src/api/client.ts:76-85`）。
  - 兜底：`?token=<...>` query string——后端 `auth.rs:177-183` 的 `extract_token` 会在 Authorization header 拿不到时回退到 `query_param("token")`。
  - Cookie：仅作 OIDC callback 后种入种子，后续读出来回填 localStorage。
- **后端校验**：`authenticate_request` 中间件（`crates/agentdash-api/src/auth.rs:103-165`）→ `AuthProvider::authenticate` → 失败时回源 `AuthSessionService`。
- **身份对象**：`AuthIdentity`（`agentdash-spi`）→ 序列化为 `CurrentUser`（`packages/app-web/src/types/index.ts:171-182`）：`auth_mode / user_id / subject / display_name / email / avatar_url / groups / is_admin / provider / extra`。

---

## 4. BYOK credentials（与登录态完全分开）

BYOK = "Bring Your Own Key"，用于让用户/管理员给单个 LLM Provider 配置自己的 API Key 或 OAuth token，**不参与平台登录**。

| 维度 | 主登录态 | BYOK |
|---|---|---|
| 存储 | `agentdash_access_token`（localStorage + cookie） | 后端 PostgreSQL，按 `(provider_id, scope=user/global, user_id)` 存（见 `crates/agentdash-domain/src/llm_provider/repository.rs`） |
| 前端 store | `useAuthStore` + `useCurrentUserStore` | `useLlmByokStore`（`packages/app-web/src/stores/llmProviderStore.ts`），和 `useLlmProviderStore` 共存 |
| API 路径 | `/api/auth/*`、`/api/me` | `/api/llm-providers/{id}/user-credential` 系列（`packages/app-web/src/api/llmProviders.ts:52-71`） |
| UI 入口 | `LoginPage` | `Settings` 页 → `UserByokSection`（`packages/app-web/src/features/settings/ui/UserByokSection.tsx`），其中 Codex 走 `OAuthLoginWizard`（`packages/app-web/src/features/settings/ui/OAuthLoginWizard.tsx`） |
| 用途 | 调 AgentDash 后端 | AgentDash 后端代用户调外部 LLM（OpenAI / Codex / etc.） |
| 验证状态 | 无（token 过期就过期） | 每条凭证有 `verification_status: unverified / verified / failed`（最近的 commit a4a8e22a 加的） |

**关键结论**：Logout 应该清"主登录态"（access token + currentUser store），**不应该**清 BYOK 凭据——后者是服务端持久化的用户配置，下次同一用户登录还要继续用。

注意：BYOK 里的 Codex OAuth wizard（`OAuthLoginWizard.tsx`）只是给 LLM Provider 拿一个外部 OAuth token，和"用户登录 AgentDash 本身"完全两件事——但 commit message 同时出现 "OAuth"、"BYOK"、"登录" 容易混淆。

---

## 5. API client / interceptors

`packages/app-web/src/api/client.ts` 是唯一的 HTTP 客户端层：

- `request<T>()`（`:25-53`）：所有 `api.get/post/put/patch/delete` 走它，自动注入 `Authorization: Bearer ...`，HTTP 错误抛 `ApiHttpError`（带 `status`）。
- `authenticatedFetch()`（`:76-85`）：原生 `fetch` 兼容签名，给 NDJSON / 流式 / blob 上传等场景用，同样注入 token。
- `setStoredToken / getStoredToken / clearStoredToken`（`:7-23, 87-119`）：唯一 token 读写入口，**logout 必须调 `clearStoredToken()`**，会同时清 localStorage 和 cookie。

**没有"401 自动跳登录"的全局拦截器**。401 处理是**散在 `AuthGate` 里的特例**：

- `packages/app-web/src/App.tsx:189-194`：`fetchCurrentUser()` 失败且 `status === 401 && needsLogin` → `clearStoredToken()`（不主动 navigate，靠 store 状态变化重渲染）。
- `packages/app-web/src/App.tsx:219-224`：currentUserError 包含 "401" 或 "认证" → 同样 clear + render LoginPage。

其余业务请求里收到 401 不会自动登出，只会冒泡为错误。

**logout 需要清的 client 副作用**：

1. `clearStoredToken()`（localStorage + cookie）
2. `useCurrentUserStore.getState().clear()`
3. `useEventStore.getState().disconnect()`（项目事件流 NDJSON，`packages/app-web/src/stores/eventStore.ts:114-132`）
4. `closeAllStreamConnections()`（`packages/app-web/src/api/streamRegistry.ts:14-24`）——session NDJSON 流注册在这
5. 桌面端：`getDesktopLocalRuntimeClient()` 启动的 local runtime（`packages/app-web/src/App.tsx:196-203` 用 token 拉起的）

`useAuthStore.logout()` 当前只做了 1+2，没做 3/4/5。

---

## 6. Existing user menu / profile UI

**当前没有用户下拉菜单 / 头像菜单**。最接近的位置：

| 组件 | 文件:行 | 描述 |
|---|---|---|
| `UserCard` | `packages/app-web/src/components/layout/workspace-layout.tsx:915-945` | 侧边栏底部常驻的用户卡片，**只显示**头像 + 姓名 + 邮箱 + Admin/企业 badge，没有任何点击行为 |
| `SidebarFooter` | `packages/app-web/src/components/layout/workspace-layout.tsx:707-850` | 侧边栏底栏容器；当前 popover 只挂了"后端连接 / 主题"两个面板 |
| `SettingsPage` 入口 | `packages/app-web/src/components/layout/workspace-layout.tsx:787-805` | 底栏齿轮按钮 → `/settings` 路由 |
| `UserAvatar` | `packages/app-web/src/components/ui/user-avatar.tsx` | 纯展示组件 |

**Logout 按钮的自然候选位置**（按"用户心智一致性"排序）：

1. 把 `UserCard` 从纯展示升级为可点击 → 弹 popover/dropdown，里面放"退出登录"
2. 在 `SidebarFooter` 现有 IconBar 加一个 `logout` icon
3. 在 `SettingsPage` 顶部"账户"区放退出登录按钮（最低交互成本，但发现性最差）

候选 1 最贴合"用户菜单"惯例，且 `UserCard` 已经天然位于底栏顶部、邻近设置入口。

---

## 7. Backend logout endpoint（已就绪）

`crates/agentdash-api/src/routes/auth_routes.rs`：

```rust
/// POST /api/auth/logout — 当前 token 失效（需要认证）
pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Result<StatusCode, ApiError>   // → 204 No Content
```

行为（`auth_routes.rs:147-163`）：

1. 从 `Authorization: Bearer ...` 或 `?token=...` 提取 token
2. 调用 `AuthSessionService::revoke_token(token)` → SQL `UPDATE auth_session SET revoked_at = now WHERE token_hash = sha256(token)`
3. 返回 `204 No Content`

路由注册（`crates/agentdash-api/src/routes.rs:73`）：在 `secured_api` 里 → 经过 `authenticate_request` 中间件，所以**调 logout 之前 token 必须仍有效**。

旁支：

- `POST /api/auth/revoke`（`auth_routes.rs:166-183`）：仅 admin（`is_admin == true`）可撤销任意 token。
- `cleanup_expired_sessions`（`session_service.rs:93-99`）：DELETE 已过期记录的 housekeeping，未发现 cron 调度入口。

**前端尚未对接 `/api/auth/logout`**。`packages/app-web/src/api/auth.ts` 里只有 `fetchLoginMetadata / postLogin / startRedirectLogin` 三个函数；`useAuthStore.logout()` 只清本地状态。后果：用户"登出"后服务端 token 仍然有效——任何拿到该 token 的人都能继续认证（直到 JWT `exp` 自然过期）。

---

## 8. Routing & redirect

- 路由库：`react-router-dom` v7（BrowserRouter）。
- 入口：`packages/app-web/src/App.tsx:308-316`，`<BrowserRouter><AuthGate><AppContent /></AuthGate></BrowserRouter>`。
- 登录守卫：`AuthGate`（`App.tsx:163-243`），**直接在 children 位置 render `<LoginPage />`**——不走 `<Navigate>`，所以 URL 不会变成 `/login`。这意味着：
  - 没有 `/login` 路由 path
  - 用户登录前后 URL 不变（停在 `/dashboard/agent` 之类的目标）
  - Logout 只需要清 token 让 `AuthGate` 重新触发 render LoginPage 的分支，**无需 navigate**
- OIDC 登录后端跳转目标：`AGENTDASH_OIDC_POST_LOGIN_REDIRECT` env，缺省 `http://127.0.0.1:5380/`（`auth_routes.rs:121-134`）。
- 已知主要业务路由（`App.tsx:265-303`）：`/dashboard/{agent,story,assets,routine}`、`/session/:sessionId`、`/workflow/:id`、`/story/:storyId`、`/settings`、`/projects/:projectId/settings`、`/dev/design-system`。

---

## 9. Tech stack snapshot

### 前端

- React 19.2 + react-router-dom 7.13 + Zustand 5.0.11 + Vite 7.3 + TypeScript 5.9 + Tailwind 4.2（`packages/app-web/package.json`）
- 所有 fetch 走自家封装 `api` / `authenticatedFetch`，没有引 axios / TanStack Query

### 后端

- Rust + axum，主 crate `agentdash-api`，依赖：
  - `agentdash-application`（应用服务，含 `AuthSessionService`）
  - `agentdash-domain`（实体 / 仓储 trait，含 `auth_session::AuthSessionRepository`）
  - `agentdash-infrastructure`（PostgreSQL 实现）
  - `agentdash-spi`（`AuthIdentity` / `AuthMode`）
  - `agentdash-plugin-api`（`AuthProvider` trait → 允许接 OIDC / 自定义企业登录插件）
- 持久化：PostgreSQL（`migrations/0001_init.sql` 含 `auth_session` 表）

### Monorepo 结构（已确认）

```
packages/
  app-web/          ← 浏览器主前端（本次 logout 涉及的核心）
  app-tauri/        ← Tauri 桌面壳
  ui/               ← 共享 UI primitives（shadcn 风）
  views/            ← 跨壳的视图组件（如 LocalRuntimeView）
  core/             ← 前端共享纯逻辑
  extension-dev/
  extension-sdk/
  extension-ui/
crates/
  agentdash-api/        ← axum 路由（auth_routes、me、...）
  agentdash-application/← 用例 / 服务（AuthSessionService）
  agentdash-domain/     ← 实体 + 仓储 trait
  agentdash-infrastructure/ ← PG 实现 + migrations
  agentdash-spi/        ← AuthIdentity 等跨层契约
  agentdash-plugin-api/ ← AuthProvider trait
  agentdash-local-tauri/, agentdash-local/, agentdash-relay/, ...
```

---

## Logout 链路盘点（小结，PRD 要消费的事实）

需要做的事 / 已就绪 vs 缺口：

| 事项 | 后端 | 前端 |
|---|---|---|
| 服务端撤销 token | ✅ `POST /api/auth/logout`（`auth_routes.rs:147-163`） | ❌ 未对接，`api/auth.ts` 没有 `postLogout` |
| 清前端 token | — | ✅ `clearStoredToken()`（`api/client.ts:20-23`，clears localStorage + cookie） |
| 清 currentUser store | — | ✅ `useCurrentUserStore.getState().clear()`（authStore.ts:75 已调） |
| 关项目事件流 | — | ⚠️ `useEventStore.disconnect()` **未在 logout 调用** |
| 关 session NDJSON / 其它流 | — | ⚠️ `closeAllStreamConnections()`（`api/streamRegistry.ts`）**未在 logout 调用** |
| 关桌面 local runtime | — | ⚠️ 启动逻辑在 `App.tsx:196-203`；停止逻辑：未发现 |
| Logout UI 入口 | — | ❌ **完全缺失** |
| 登出后回登录页 | — | ✅ 不用 navigate，靠 `AuthGate` 状态机重渲染 LoginPage |
| 登出后行为可见性 | — | ⚠️ 需要决定：show toast / redirect / show "已登出" 状态 |

## Caveats / Not Found

- 没看到任何"自动清理过期 auth_session"的后台 job；`cleanup_expired_sessions` 函数存在（`session_service.rs:93-99`）但搜不到调用方，可能是历史保留。PRD 不需要本次解决。
- 没找到任何 axios / fetch 全局 401 拦截器；当前的 401 处理只在 `AuthGate` 内的 `/api/me` 一处生效，**业务请求 401 不会自动登出**。Logout 实现可以不依赖也不引入这个机制——但应该意识到 token 撤销后下一次业务请求会失败而不会自动跳转，需要 PRD 明确策略。
- `app-tauri`（Tauri 壳）下的登录/登出行为没有单独检查；`App.tsx:196-203` 暗示桌面端有额外的 local runtime token 注入路径（`ensureDesktopLocalRuntimeStarted(token)`），logout 时是否需要 stop 这个 runtime 没有明确答案。
- `LoginResponse` 类型定义里有 `identity` 字段（`types/index.ts:159-162`），但 `authStore.login()` 拿到 response 后只用了 `response.access_token`，紧跟着另起一次 `/api/me` 请求来填 currentUser（`authStore.ts:42-45`）——这个冗余不影响 logout 设计但值得在 PRD 里注意。
- 没有看到 `remember_me` / 多设备会话管理 UI。`auth_session` 表理论上可以列出"我所有的活跃会话"，但前端没暴露。
