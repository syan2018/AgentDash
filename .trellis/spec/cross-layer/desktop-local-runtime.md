# Desktop Local Runtime

Tauri 桌面端把 Web Dashboard、本机 runtime 管理面板和桌面托管 API 收敛在同一个应用进程中。本文档约束跨 Rust/Tauri/React 的 command、HTTP authority、profile 和打包入口。

## Scenario: Tauri 桌面端托管本机 Dashboard 与 Local Runtime

### 1. Scope / Trigger

- Trigger: 桌面端需要在无外部 cloud server 的情况下打开完整 Dashboard，同时管理内嵌的 `agentdash-local` runtime。
- Scope: `agentdash-local-tauri` 作为薄壳持有 `LocalRuntimeManager`，通过 Tauri command 暴露 runtime/profile/MCP/log 操作，并在独立 Tokio runtime 线程启动 `agentdash-api`。
- Boundary: 桌面端 Dashboard 不直接访问 Rust 内存态；Dashboard 仍通过 HTTP API 访问 `agentdash-api`。Local Runtime 设置面板才通过 Tauri `invoke()` 访问本机 runtime manager。

### 2. Signatures

Rust API server reusable entry:

```rust
pub struct ApiServerOptions {
    pub service_name: String,
    pub host: String,
    pub port: u16,
    pub max_connections: u32,
}

impl ApiServerOptions {
    pub fn from_env() -> anyhow::Result<Self>;
    pub fn desktop_localhost(port: u16) -> Self;
}

pub struct ApiServerReady {
    pub addr: String,
    pub origin: String,
    pub database_url: String,
}

pub async fn build_server(
    plugins: Vec<Box<dyn AgentDashPlugin>>,
    options: ApiServerOptions,
) -> anyhow::Result<ApiServer>;
```

Tauri commands:

```text
desktop_api_snapshot() -> DesktopApiSnapshot
profile_load() -> LocalRuntimeProfile | null
profile_save(profile: LocalRuntimeProfile) -> LocalRuntimeProfile
profile_delete() -> void
runtime_snapshot() -> LocalRuntimeStatus | null
runtime_start(request: RuntimeStartRequest) -> LocalRuntimeStatus
runtime_stop() -> void
runtime_restart() -> LocalRuntimeStatus
logs_tail(limit?: number) -> LocalLogEvent[]
logs_clear() -> void
mcp_servers_load(root: PathBuf) -> McpLocalServerEntry[]
mcp_servers_save(root: PathBuf, servers: McpLocalServerEntry[]) -> void
mcp_server_probe(server: McpLocalServerEntry) -> McpProbeResult
```

Shared TypeScript client port:

```typescript
export interface LocalRuntimeClient {
  profileLoad(): Promise<LocalRuntimeProfile | null>
  profileSave(profile: LocalRuntimeProfile): Promise<LocalRuntimeProfile>
  profileDelete(): Promise<void>
  runtimeSnapshot(): Promise<LocalRuntimeStatus | null>
  runtimeStart(request: RuntimeStartRequest): Promise<LocalRuntimeStatus>
  runtimeStop(): Promise<void>
  runtimeRestart(): Promise<LocalRuntimeStatus>
  logsTail(limit?: number): Promise<LocalLogEvent[]>
  logsClear(): Promise<void>
  mcpServersLoad(root: string): Promise<McpLocalServerEntry[]>
  mcpServersSave(root: string, servers: McpLocalServerEntry[]): Promise<void>
  mcpServerProbe(server: McpLocalServerEntry): Promise<McpProbeResult>
}
```

### 3. Contracts

- Desktop API 默认使用 `ApiServerOptions::desktop_localhost(3001)`，绑定 `127.0.0.1:3001`，`service_name = "agentdash_desktop_api"`。
- `desktop_api_snapshot` 响应使用 snake_case：`state`、`origin`、`message`、`database_url`。`state` 只能是 `starting | running | error | stopped`。
- DashboardHost 必须优先读取 `desktop_api_snapshot().origin`，再请求 `${origin}/api/health`，确认 ready 后才渲染 Web Dashboard。
- `LocalRuntimeProfile` 持久化在 Tauri app config dir 下的 `desktop-runtime-profile.json`，字段使用 snake_case，包含 `server_url`、`access_token`、`profile_id`、`machine_id`、`machine_label`、`legacy_machine_ids`、`backend_id`、`relay_ws_url`、`name`、`accessible_roots`、`executor_enabled`、`auto_start`。
- 机器级身份由 `agentdash-local` runtime library 负责识别、生成和持久化；Tauri / dev scripts 只能调用 local library 或 `agentdash-local machine-identity` 获取结果，不得维护第二套 machine identity 文件。
- `backend_id`、`relay_ws_url` 和 relay token 必须来自 server ensure/claim 响应；Tauri/renderer 不允许自行拼接或发明 server 侧 runtime 身份。
- `access_token` 可以为空。Personal auth / 本地开发模式下，server 仍可通过自身认证 provider 解析当前用户；Tauri 只有在 token 非空时才发送 Bearer header。
- server ensure API 使用 `machine_id + share_scope_kind + share_scope_id + capability_slot` 定位 local backend。个人本机是 `scope.kind=user`、`visibility=private`；未来共享本机使用同一模型扩展 `project/system` scope。
- server ensure API 必须把 `machine_label` 及其大小写/`.local` 变体纳入 legacy identity 候选；repository 命中 legacy backend 时应合并重复 local backend row，避免同一台机器在 Web/Tauri 登录后出现多个 personal runtime。
- `scripts/dev-joint.js` 必须复用同一条 ensure/claim 协议。开发脚本必须通过 `agentdash-local machine-identity` 读取 local runtime 自己识别到的 `machine_id` 与 `machine_label`，不得直接调用 `/api/backends` 创建 local backend，也不得提供 `--backend-id` 让调用方绕过 server 生成规则。
- 开发期 `scripts/dev-desktop.js` 不得注入 machine identity 路径；Tauri 壳应复用 `agentdash-local` crate 的机器身份逻辑，确保 Web 联合调试和桌面调试看到同一台 local runtime。
- `device_id` 仅作为旧 profile/backend 的 legacy merge 输入存在；新前端/Tauri 请求不得生成或提交新的 `device_id`。
- Local Runtime UI 不直接 import Tauri API；它只依赖 `@agentdash/core` 的 `LocalRuntimeClient` port。`app-tauri` 负责把 `invoke()` 适配成 client。
- `packages/app-web` 只导出 `App`，`packages/app-tauri` 复用该入口作为 Dashboard 页，不能复制 Web Dashboard 组件树。
- `@agentdash/ui/styles.css` 是 Web/Tauri 共享的唯一全局样式入口，承载 Tailwind v4 theme、base layer、component layer 和第三方渲染样式；`app-web`、`app-tauri`、`views` 不能再各自维护全局壳样式。
- 桌面打包入口必须可复现：`pnpm run desktop:build` 生成 release exe，`pnpm run desktop:bundle` 生成 Windows NSIS installer。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| 桌面 API 尚未启动 | DashboardHost 展示 starting 诊断状态，并轮询 snapshot |
| 桌面 API 端口占用或迁移失败 | `desktop_api_snapshot.state = error`，UI 展示错误信息 |
| `/api/health` 非 2xx | DashboardHost 不渲染 Dashboard，展示健康检查失败 |
| profile 文件不存在 | `profile_load()` 返回 `null` |
| profile JSON 解析失败 | Tauri command 返回错误字符串，不吞掉错误 |
| runtime 仍有 Running session | `runtime_restart()` 拒绝重启 |
| MCP probe 连接失败 | 返回 `McpProbeResult { ok: false, ... }`，不升级成 command error |
| 日志清空 | 清空后追加一条 `runtime` info 日志说明操作完成 |
| Tauri CLI 缺失 | 仓库依赖 `@tauri-apps/cli`，不能要求开发者全局安装 |

### 5. Good/Base/Bad Cases

- Good: `agentdash-api` 抽出 `build_server`，Tauri 只选择 host/port/service_name，不复制 Axum route/DI/migration。
- Good: 桌面 Local Runtime 页通过 `LocalRuntimeClient` 适配 Tauri commands，后续 Web/测试环境可替换 client。
- Good: Web 与 Tauri 只 import `@agentdash/ui/styles.css` 作为全局 CSS，组件视觉差异通过共享 UI primitives 和 Tailwind token 消除。
- Good: `tauri.conf.json` 的 `beforeBuildCommand` 构建 `packages/app-tauri`，避免 bundle 使用过期 dist。
- Base: Dashboard 在 Tauri 中仍使用 HTTP API，这是 Dashboard 数据 authority；Local Runtime 设置才使用 process-local command。
- Bad: 在 `app-tauri` 复制 `packages/app-web/src` 下的 Dashboard 组件。
- Bad: 从 `app-web` 导出样式给桌面端，或在 `app-tauri` / `views` 追加全局 CSS 来修补桌面样式。
- Bad: Dashboard 直接读取 `LocalRuntimeManager` 或通过 Tauri command 绕过 `agentdash-api` 的 Repository/API 契约。
- Bad: 用 hostname、随机 UUID 或当前登录用户直接拼 `backend_id`。server 侧 backend 身份必须由 ensure API 根据稳定 machine/scope 决定。
- Bad: 开发脚本为了省事直接 POST `/api/backends`，或把 `local-dev-1` 一类固定 `backend_id` 写进启动参数。
- Bad: `pnpm dev` / `pnpm dev:desktop` / Tauri 各自生成或指定开发机 `machine_id`，导致同一用户看到多个“本机” personal backend。
- Bad: 依赖本机全局 `cargo tauri`；仓库脚本应使用 `pnpm exec tauri`。

### 6. Tests Required

- Typecheck: `pnpm run desktop:check`。
- Frontend build: `pnpm --filter app-web build`。
- Frontend build: `pnpm --filter app-tauri build`。
- Rust check: `cargo check -p agentdash-infrastructure -p agentdash-api -p agentdash-local-tauri`。
- Desktop release build: `pnpm run desktop:build`。
- Windows bundle: `pnpm run desktop:bundle`，确认 `target/release/bundle/nsis/AgentDash_0.1.0_x64-setup.exe` 生成。
- Smoke: `cargo run -p agentdash-local-tauri` 后，`http://127.0.0.1:3001/api/health` 返回 200，Dashboard 页能进入 Web Dashboard。

### 7. Wrong vs Correct

#### Wrong

```typescript
import WebDashboardApp from '../../app-web/src/App'
import { invoke } from '@tauri-apps/api/core'

export function LocalRuntimePage() {
  // 组件内部直接调用 invoke，无法在非 Tauri 环境复用或测试。
}
```

#### Correct

```typescript
import { LocalRuntimeView } from '@agentdash/views'
import type { LocalRuntimeClient } from '@agentdash/core'

export function LocalRuntimePage({ client }: { client: LocalRuntimeClient }) {
  return <LocalRuntimeView client={client} />
}
```

这样 Tauri 适配层、共享视图和 Web Dashboard 入口保持各自边界清晰，后续扩展 profile、token、runtime recovery 或测试替身时不会把桌面壳和业务 UI 重新耦合。
