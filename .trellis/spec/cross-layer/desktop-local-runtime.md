# Desktop Local Runtime

Tauri 桌面端把 Web Dashboard、本机 runtime 管理面板和桌面托管 API 收敛在同一个应用进程中。本文档约束跨 Rust/Tauri/React 的 command、HTTP authority、profile 和打包入口。

## Scope

- `agentdash-local-tauri` 作为薄壳持有 `LocalRuntimeManager`，通过 Tauri command 暴露 runtime/profile/MCP/log 操作，并在独立 Tokio runtime 线程启动 `agentdash-api`。
- Dashboard 不直接访问 Rust 内存态；仍通过 HTTP API 访问 `agentdash-api`。Local Runtime 设置面板才通过 Tauri `invoke()` 访问本机 runtime manager。

## 关键类型

- **Rust**：`ApiServerOptions`（`agentdash-api/src/lib.rs`）+ `build_server()` 可复用入口
- **Tauri commands**：profile / runtime / logs / MCP / open_external_url（定义在 `agentdash-local-tauri`）
- **TS port**：`LocalRuntimeClient`（`@agentdash/core`），Tauri 适配层实现 `invoke()` 绑定

## 核心约束

### API 与 Dashboard

- Desktop API 默认 `127.0.0.1:3001`，`service_name = "agentdash_desktop_api"`
- `desktop_api_snapshot` 的 `state` 只能是 `starting | running | error | stopped`
- DashboardHost 必须先确认 `/api/health` ready 后才渲染 Web Dashboard
- `packages/app-web` 只导出 `App`，`app-tauri` 复用该入口，不能复制组件树

### 机器身份

- 机器级身份由 `agentdash-local` runtime library 负责识别、生成和持久化
- Tauri / dev scripts 只能调用 local library 或 `agentdash-local machine-identity` 获取
- `backend_id`、`relay_ws_url` 和 relay token 必须来自 server ensure/claim 响应
- server ensure API 使用 `machine_id + share_scope_kind + share_scope_id + capability_slot` 定位 local backend
- server ensure API 只能使用稳定 `machine_id` 与显式 `legacy_machine_ids` 做身份匹配；`machine_label` / hostname 只用于展示
- profile merge 只使用稳定 `machine_id` 与显式 legacy ids；新请求不得生成新的 `device_id`
- `scripts/dev-joint.js` 必须复用同一条 ensure/claim 协议，通过 `agentdash-local machine-identity` 读取身份

### Profile

- `LocalRuntimeProfile` 持久化在 Tauri app config dir 下的 `desktop-runtime-profile.json`（snake_case）
- 每次 profile load/save/start 都必须用 `agentdash-local` 机器身份覆盖 canonical machine id
- `access_token` 可以为空，server 在无 token 时通过自身认证 provider 解析当前用户
- `workspace_roots` 表达显式登记的 workspace root 集合；为空时不构成异常，也不限制本机目录浏览。执行类能力仍以 session `mount_root_ref` 为当前 workspace root 边界。
- 本机目录浏览是 setup 选择器能力，默认允许全盘浏览；workspace detect/register 成功后产生目录事实，session prompt / file tool / shell 才进入执行边界。

### Relay Prompt / Event Lifecycle

- Cloud relay connector 在发送 `command.prompt` 前注册 session event sink，原因是 local runtime 可以在 `response.prompt` 前推送 session notification 或 terminal state。
- Relay executor discovery 读取 backend registry 维护的在线 executor 快照；`AgentConnector::list_executors()` 是同步接口，不能在同步 discovery 路径里临时 `block_on` registry 的 async 状态查询。
- Backend registry 的 pending command 归属到具体 `backend_id`；backend 断连时释放该 backend 的 pending sender，让调用方立即收到 response dropped，而不是等待 command timeout。
- Cloud 侧用 `backend_execution_leases` 记录 relay session turn 对 backend 的执行占用。`runtime_health` 只表达连接健康，workspace inventory / binding 只表达目录事实，执行空闲/忙碌由 active lease 投影。
- Session launch 负责把 backend selection intent 解析成已 claim 的 backend execution placement，并把 `backend_id + lease_id + selection_mode` 放进 connector `ExecutionContext`。Relay connector 不再从 VFS mount 自行猜测执行 backend。
- Relay session sink 记录 `session_id -> backend_id + lease_id + sender`，原因是 cancel、terminal release 与 backend disconnect cleanup 都必须落到实际承载该 session 的 backend，而不是广播或重新猜测。
- Relay prompt 自动选择 backend 时先筛选在线且提供目标 executor 的 backend，再按 active lease count 升序与 backend_id 稳定排序；capacity / weight 不属于第一版调度输入。
- `/backends/runtime-summary` 是前端展示执行空闲/忙碌与可分配状态的汇总投影；前端消费该投影，不从 runtime health 或 executor snapshot 自行推断。
- Local runtime 的 session notification forwarder 按 `session_id` 唯一运行；同一 relay session 的 follow-up prompt 复用现有 forwarder，保证同一条 session event 只有一个 relay 转发路径。
- Relay protocol 顶层信封保留在 `agentdash-relay/src/protocol.rs`；握手、心跳和 capability discovery payload 位于 `agentdash-relay/src/protocol/handshake.rs`，prompt / discovery / workspace / tool / VFS materialization / terminal / session event / MCP payload 位于 `agentdash-relay/src/protocol/` 对应子模块。顶层信封和子协议 payload 分离，原因是 wire format 必须集中稳定，而各子协议会按本机能力独立演进。

### Extension Artifact Cache

- `agentdash-local` 通过后端 archive download API 获取 Project scoped extension package artifact。
- cache key 使用 `artifact_id + archive_digest`，原因是同一 artifact 重新发布或 digest 改变时必须形成新的本机缓存目录。
- 下载后必须校验 archive sha256 digest，再把 `.agentdash-extension.tgz` 解包到可清理 cache 目录。
- 解包只接受 archive 内相对普通文件路径；Extension Host 读取 cache 中的 package 内容，不在安装路径执行 npm/pnpm install 或 package lifecycle scripts。

### Local TS Extension Host

- `agentdash-local` 管理 Node-based extension host 子进程，通过 stdio JSON line 协议执行 activate / reload / invoke / health。
- Extension bundle 在受限 VM context 中加载 self-contained ESM，原因是插件包来自可安装产物，运行面需要与本机 runtime 主进程隔离。
- `api.local.getProfile()` 由 Rust host API facade 返回 username、platform、arch、backend/project/session 与 workspace root 摘要，原因是本机 profile 是 local runtime 的事实源。
- 本机 profile API 按 manifest `local_profile` 权限或 action `local.profile.read` 权限裁决，原因是插件贡献声明是 runtime surface 与本机能力之间的授权边界。
- packaged mode 直接消费 `ExtensionArtifactCacheEntry.unpacked_dir`，原因是 artifact cache 已完成 archive digest 校验与安全解包。
- action exception 和 host process exit 投影为 host 调用错误，原因是 extension host 故障应隔离在插件执行面内，保留 `agentdash-local` 主进程生命周期。

### 样式与依赖

- `@agentdash/ui/styles.css` 是 Web/Tauri 共享的唯一全局样式入口
- Local Runtime UI 不直接 import Tauri API，只依赖 `@agentdash/core` 的 `LocalRuntimeClient` port
- 桌面端打开外部网页时通过 `open_external_url` command（仅允许 http/https）

## Validation Matrix

| Condition | Required behavior |
|---|---|
| API 尚未启动 | DashboardHost 展示 starting 状态并轮询 |
| API 端口占用 | `state = error`，UI 展示错误 |
| `/api/health` 非 2xx | 不渲染 Dashboard |
| profile 不存在 | `profile_load()` 返回 `null` |
| runtime 有 Running session | `runtime_restart()` 拒绝 |
| MCP probe 失败 | 返回 `{ ok: false }`，不升级成 command error |
| Tauri CLI 缺失 | 仓库依赖 `@tauri-apps/cli`，不要求全局安装 |

## 禁止模式

- 在 `app-tauri` 复制 Web Dashboard 组件
- Dashboard 绕过 `agentdash-api` 的 Repository/API 契约
- 用 hostname / 随机 UUID 拼 `backend_id`
- 开发脚本直接 POST `/api/backends` 或写死 `backend_id`
- 多个入口各自生成 `machine_id`
- 依赖全局 `cargo tauri`（应使用 `pnpm exec tauri`）
- 在 `app-tauri` / `views` 追加全局 CSS
