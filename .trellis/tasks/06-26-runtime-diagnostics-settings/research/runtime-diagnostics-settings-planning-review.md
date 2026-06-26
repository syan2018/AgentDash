# Research: runtime diagnostics settings planning review

- Query: 为子任务“运行状态诊断与设置体验”审阅现有计划，补全 design/implement review、handoff contract 与小步提交建议。
- Scope: internal
- Date: 2026-06-26

## Findings

### Files Found

- `.trellis/tasks/06-26-runtime-diagnostics-settings/prd.md` - 已定义目标：区分 Cloud API、Desktop API、Local Runtime/Runner、WebSocket relay，并要求注册来源、日志 tail、重启入口和 token 脱敏。
- `.trellis/tasks/06-26-runtime-diagnostics-settings/design.md` - 已有高层结构，但状态事实源、DTO 字段、注册来源、runner/desktop 依赖和错误文案矩阵仍需细化。
- `.trellis/tasks/06-26-runtime-diagnostics-settings/implement.md` - 已有 checklist，但缺少按依赖拆分的小步提交、contract 生成、测试边界和跨任务 handoff gate。
- `.trellis/tasks/06-26-runtime-diagnostics-settings/implement.jsonl` - 已引用 desktop local runtime、frontend type safety/design language、backend diagnostics 四类关键 spec。
- `.trellis/tasks/06-26-runtime-diagnostics-settings/check.jsonl` - 已引用 frontend quality 与 cross-layer thinking guide；建议补充本 research 文件作为 check 上下文。
- `crates/agentdash-local-tauri/src/main.rs` - Tauri command、Desktop API snapshot、profile、local runtime start/restart/logs 命令入口。
- `crates/agentdash-local/src/runtime.rs` - LocalRuntimeManager snapshot/log buffer/restart 行为与脱敏实现。
- `crates/agentdash-local/src/ws_client.rs` - relay WebSocket 连接、register/register_ack 与重连循环。
- `packages/core/src/local-runtime/index.ts` - 前端 LocalRuntimeClient port 与 LocalRuntimeStatus/LocalLogEvent 类型。
- `packages/app-tauri/src/runtimeApi.ts` - Tauri invoke 到 LocalRuntimeClient 的适配层。
- `packages/app-tauri/src/App.tsx` - Desktop API host readiness gate 与 `desktop_api_snapshot` 消费。
- `packages/app-web/src/desktop/localRuntimeBridge.ts` - Web 侧读取 Tauri 注入的 LocalRuntimeClient 与 auto-start profile。
- `packages/views/src/local-runtime/LocalRuntimeView.tsx` - 当前本机运行时设置面板，已包含 runtime start/restart/stop、日志 tail/清空/复制、MCP 配置。
- `packages/app-web/src/features/settings/ui/SettingsPageContent.tsx` - 设置页 panel 路由，desktop-only local runtime panel 挂载点。
- `packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx` - 后端管理状态展示，已消费 `/backends` 与 `/backends/runtime-summary`。
- `packages/app-web/src/stores/eventStore.ts` 与 `packages/app-web/src/api/eventStream.ts` - Project NDJSON event stream 连接状态；不是 relay WebSocket 状态，但可作为“Cloud API event stream”展示输入。
- `packages/app-web/src/generated/backend-contracts.ts` - 当前 generated backend DTO，尚无注册来源字段。
- `crates/agentdash-api/src/routes/backends.rs` 与 `crates/agentdash-application/src/backend/management.rs` - `/api/local-runtime/ensure` claim/ensure 入口与 backend record 写入。
- `crates/agentdash-domain/src/backend/entity.rs` - BackendConfig/RuntimeHealth/LocalBackendClaim 当前领域字段。

### Code Patterns

- Desktop API snapshot 事实源在 Tauri 壳内，字段为 `state/origin/message/database_url`，状态枚举为 `starting | running | error | stopped`：`crates/agentdash-local-tauri/src/main.rs:46`, `crates/agentdash-local-tauri/src/main.rs:55`, `crates/agentdash-local-tauri/src/main.rs:304`。
- Tauri 已暴露 `runtime_snapshot`、`runtime_restart`、`logs_tail`、`logs_clear`，并在 invoke handler 注册：`crates/agentdash-local-tauri/src/main.rs:193`, `crates/agentdash-local-tauri/src/main.rs:202`, `crates/agentdash-local-tauri/src/main.rs:286`, `crates/agentdash-local-tauri/src/main.rs:294`, `crates/agentdash-local-tauri/src/main.rs:651`。
- Desktop claim 通过 `/api/local-runtime/ensure` 获取 `backend_id/relay_ws_url/auth_token`，随后构造 LocalRuntimeConfig；这些事实不能本地拼接：`crates/agentdash-local-tauri/src/main.rs:343`, `crates/agentdash-local-tauri/src/main.rs:408`, `crates/agentdash-local-tauri/src/main.rs:501`。
- LocalRuntimeStatus 当前只包含 runtime manager 视角的 `state/backend_id/name/workspace_roots/executor_enabled/mcp_server_count/message`，尚无 relay 连接状态、最近错误、注册来源、连接目标：`crates/agentdash-local/src/runtime.rs:69`, `packages/core/src/local-runtime/index.ts:3`。
- LocalRuntimeManager restart 会拒绝正在运行/取消中的 session，计划中的重启按钮需要在 UI 文案中解释这个错误：`crates/agentdash-local/src/runtime.rs:259`。
- Local runtime 日志是 500 条有界 ring buffer，`logs_tail` 支持 limit，`logs_clear` 清空后 Tauri 重新记录“已清空本机日志”：`crates/agentdash-local/src/runtime.rs:138`, `crates/agentdash-local/src/runtime.rs:295`, `crates/agentdash-local-tauri/src/main.rs:294`。
- 当前日志脱敏只覆盖 `token=`, `access_token=`, `refresh_token=` 这类 marker，尚不覆盖 `Authorization: Bearer ...`、JSON 字段、query `?token=` 外的大小写变体：`crates/agentdash-local/src/runtime.rs:559`。
- relay WebSocket URL 当前在 local runtime 内拼成 `cloud_url?token=...`，连接失败只走 `diag!`，没有进入 LocalRuntimeStatus 或 LocalLogEvent：`crates/agentdash-local/src/ws_client.rs:62`, `crates/agentdash-local/src/ws_client.rs:67`, `crates/agentdash-local/src/ws_client.rs:78`。
- Web 设置页当前将 local runtime 作为 desktop-only panel 插入，只有 Tauri host 才显示：`packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:52`, `packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:121`, `packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:231`。
- LocalRuntimeView 已有运行状态概览、重启/停止、profile、自启动、日志刷新/复制/清空入口：`packages/views/src/local-runtime/LocalRuntimeView.tsx:295`, `packages/views/src/local-runtime/LocalRuntimeView.tsx:320`, `packages/views/src/local-runtime/LocalRuntimeView.tsx:469`, `packages/views/src/local-runtime/LocalRuntimeView.tsx:541`。
- 当前“自动启动”只在 local runtime profile 中表达，不包含“启动到托盘”或 OS login-item 自启动设置命令：`crates/agentdash-local-tauri/src/main.rs:114`, `packages/views/src/local-runtime/LocalRuntimeView.tsx:476`。
- `/backends` 已将 runtime_health 与在线 registry 合并，`/backends/runtime-summary` 已用于空闲/忙碌/可分配展示：`crates/agentdash-api/src/routes/backends.rs:81`, `packages/app-web/src/stores/coordinatorStore.ts:35`, `packages/app-web/src/types/acp.ts:91`。
- generated backend DTO 目前没有 `registration_source` 或 runner token 来源字段，只有 backend type/scope/last_claimed_at/runtime_health：`packages/app-web/src/generated/backend-contracts.ts:12`, `packages/app-web/src/generated/backend-contracts.ts:14`, `packages/app-web/src/generated/backend-contracts.ts:22`。
- BackendConfig 领域字段同样没有注册来源；`last_claimed_at` 注释只说明 Desktop ensure/claim 最近时间：`crates/agentdash-domain/src/backend/entity.rs:10`, `crates/agentdash-domain/src/backend/entity.rs:40`。
- relay server 以 query token 认证 backend，并校验首条 register 的 backend_id 与 token 绑定 backend 一致；这适合作为 runner handoff 中“token 类型/来源”之外的 relay 认证事实：`crates/agentdash-api/src/relay/ws_handler.rs:25`, `crates/agentdash-api/src/relay/ws_handler.rs:91`, `crates/agentdash-api/src/relay/ws_handler.rs:727`。

### Related Specs

- `.trellis/spec/cross-layer/desktop-local-runtime.md:13` 要求 Tauri commands 覆盖 profile/runtime/logs/MCP/open_external_url。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:20` 与 `:21` 固定 Desktop API 默认 origin 与 `desktop_api_snapshot` 状态枚举。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:29` 要求 `backend_id`、`relay_ws_url` 和 relay token 来自 server ensure/claim。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:33` 要求 profile 保存 server/profile/workspace roots/backend claim result/启动偏好。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:55` 要求前端消费 `/backends/runtime-summary` 展示执行空闲/忙碌，不自行从 runtime health 推断。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:318` 要求 Local Runtime UI 依赖 `@agentdash/core` 的 `LocalRuntimeClient` port，不直接 import Tauri API。
- `.trellis/spec/frontend/type-safety.md:10` 与 `:11` 要求 snake_case 直映射、generated wire 单源。
- `.trellis/spec/frontend/type-safety.md:36` 与 `:41` 要求跨层 enum/DTO 来自 generated contract，feature view model 由 DTO 显式转换。
- `.trellis/spec/frontend/type-safety.md:63` 要求新增或修改跨层 DTO 后跑 `pnpm run contracts:check`。
- `.trellis/spec/frontend/design-language.md:12`, `:14`, `:113`, `:125`, `:128` 约束语义 token、primitive、Card/Notice/StatusDot 使用。
- `.trellis/spec/backend/diagnostics-guidelines.md:17` 区分平台过程诊断与领域数据，不能把 session events/runtime health 混进诊断日志。
- `.trellis/spec/backend/diagnostics-guidelines.md:23`, `:60`, `:85`, `:103`, `:109` 约束 diag!、Subsystem、关联字段、local-tauri 不暴露 API 诊断缓冲、API diagnostics 查询端点。

### External References

- 未使用外部资料；本次审阅完全基于仓库内 task/spec/code/generated contract。

## Planning Review

### 1. 当前计划缺口/风险（按优先级）

1. **P0 状态事实源边界还不够硬。** 计划应明确四层状态分别来自哪里：Cloud API/事件流来自 Web HTTP/NDJSON client，Desktop API 来自 `desktop_api_snapshot`，Local Runtime/Runner 来自 Tauri `runtime_snapshot` 与后端 `/backends`/`runtime-summary`，relay 连接来自 runner/local runtime 显式上报的 relay connection snapshot。不要让 UI 从日志或 `backend.online` 反推 relay 连接状态。
2. **P0 缺少统一诊断 DTO/snapshot 设计。** 现有 LocalRuntimeStatus 不足以表达 Desktop API、Cloud API、relay last_error、registration_source、redacted targets、runner registration token 来源。design 需要给出 DTO 字段、枚举、null 语义、刷新频率和错误归属。
3. **P0 注册来源当前没有后端/generated 字段。** `BackendWithStatusResponse` 和 `BackendConfig` 未暴露 `registration_source`。如果 runner 任务会新增 registration token 注册路径，本任务需要等 handoff 或在 design 写成依赖项，避免前端用 backend type/scope/last_claimed_at 猜来源。
4. **P0 relay 连接状态目前只在 ws_client 诊断日志中出现。** `ws_client` 的连接失败/重连/register_ack 没有进入 LocalRuntimeStatus；如果要准确显示 relay 可用性，需要 runner/local runtime 提供结构化 `relay_connection`，至少包括 state、last_connected_at、last_error、retry_count/next_retry_at。
5. **P1 日志脱敏要求需要扩展。** 当前脱敏只处理 `token=`, `access_token=`, `refresh_token=` marker；设计应要求覆盖 URL query token、Bearer header、JSON 字段（`token/access_token/refresh_token/auth_token/relay_token/registration_token`）和大小写变体，并把“复制/导出”也走同一脱敏函数。
6. **P1 桌面设置能力与 OS 集成未定义。** PRD 要求自启动、启动到托盘、启动后自动连接 runtime。当前只有 profile `auto_start`；缺少 OS login item、自启动到托盘、关闭行为/托盘行为、设置持久化和 Tauri command 合约。
7. **P1 UI 分区和文案矩阵需要补全。** 当前 LocalRuntimeView 是一个本机 runtime 管理面板，没有 Cloud API、Desktop API、relay、runner registration source 的独立状态区。design 应约定每层的状态词、恢复动作和错误文案，避免“本机 API/本机 runtime/runner”混用。
8. **P1 runner 与 desktop 任务依赖未落成 gate。** PRD notes 提到依赖稳定状态快照，但 implement 没有写“没有 handoff 不开始哪一步”。建议把 runner/Windows desktop handoff contract 写进 design/implement，并在 checklist 中设为前置检查。
9. **P2 generated contract 与 type-safety 验证缺失。** 若新增后端 DTO，implement 需要包含 contract crate 更新、生成 TS、service/view model 显式转换、`pnpm run contracts:check`。
10. **P2 现有 SettingsPage 设计语言有遗留问题。** Scope tabs 使用 `rounded-full` 和字面 SVG，LocalRuntimeView 中也有手写符号按钮；本任务若触达这些区域，应按 design-language 使用 primitive/lucide 图标/8px radius/语义 token，但不要顺手重构无关范围。

### 2. 推荐的更完整 design 结构

建议把 `design.md` 扩为以下结构：

1. **目标与非目标**
   - 目标：在设置页提供“运行状态诊断”事实视图，解释 Cloud API、Desktop API、Local Runtime/Runner、relay 的当前状态、最近错误、恢复动作。
   - 非目标：不替代 session events、runtime health、Context Audit；不把平台日志当领域事件。

2. **状态事实源**
   - `cloud_api`: Web HTTP health/me/settings 请求与 Project event stream lifecycle；表达 Dashboard 访问云端 API 或 Desktop API 的 HTTP 可达性。
   - `desktop_api`: Tauri `desktop_api_snapshot`，仅桌面宿主可用，状态枚举 `starting | running | error | stopped`。
   - `local_runtime`: Tauri `runtime_snapshot` 与 LocalRuntimeManager command 结果，表达桌面托管 local runtime 进程状态。
   - `runner`: 云端 `/backends` + `/backends/runtime-summary` + runner handoff snapshot，表达独立 runner/remote backend 的在线与执行可分配状态。
   - `relay_connection`: runner/local runtime 结构化上报，表达 WebSocket relay handshake/connected/reconnecting/error，与 backend registry online 相关但不是同一事实。

3. **DTO / snapshot contract**
   - `RuntimeDiagnosticsSnapshot` 建议字段：
     - `generated_at: string`
     - `cloud_api: ApiLayerStatus`
     - `desktop_api: DesktopApiLayerStatus | null`
     - `local_runtime: LocalRuntimeLayerStatus | null`
     - `runner: RunnerLayerStatus | null`
     - `relay_connection: RelayConnectionStatus | null`
     - `registration: RuntimeRegistrationStatus | null`
     - `logs: LocalLogEvent[]`
     - `settings: DesktopRuntimeSettings | null`
   - `LayerState`: `unknown | checking | healthy | degraded | unavailable | disabled`
   - `RelayConnectionState`: `not_configured | connecting | registered | reconnecting | disconnected | error`
   - `RegistrationSource`: `desktop_access_token | runner_registration_token`
   - DTO 全部 snake_case；后端/API DTO 进入 generated contracts，Tauri-only DTO 放在 `@agentdash/core/local-runtime` port，UI view model 单独转换。

4. **注册来源**
   - Desktop access token 来源：用户登录 token 调 `/api/local-runtime/ensure`，server 返回 backend claim result 与 relay token；UI 展示“桌面登录授权”，不展示 token。
   - Runner registration token 来源：runner 使用 registration token 领取/注册 backend；UI 展示“Runner 注册令牌”，不展示 token。
   - `registration` 应至少包含 `source`, `backend_id`, `profile_id`, `machine_id`, `machine_label`, `share_scope_kind`, `share_scope_id`, `capability_slot`, `claimed_at`, `registered_at`, `last_seen_at`。
   - 来源必须由 server/runner 明确返回，不从 endpoint、scope 或 backend_type 推断。

5. **日志与脱敏**
   - 所有 local runtime logs、copy/export、recent error 都经过同一个脱敏函数。
   - 脱敏覆盖：query `token/access_token/refresh_token/auth_token/relay_token/registration_token`，Bearer token，JSON/string field，大小写变体。
   - 日志只表达平台过程诊断；session events、shell output、Context Audit 不进入此日志区。
   - UI 显示最近 N 条 tail，支持刷新、复制、清空；若支持导出，导出也必须使用脱敏后数据。

6. **命令 contract**
   - 已有：`runtime_snapshot`, `runtime_start`, `runtime_stop`, `runtime_restart`, `logs_tail`, `logs_clear`, `desktop_api_snapshot`。
   - 需要补齐或确认：`desktop_runtime_settings_load/save`（auto launch、start minimized/to tray、auto connect runtime）、`runtime_diagnostics_snapshot`（可选聚合命令，减少 UI 多源拼装）、`runner_restart` 是否存在以及是否 desktop 管理范围。
   - `runtime_restart` 遇到 active session 必须返回可显示错误；UI 文案解释“当前有会话正在运行，结束后再重启”。

7. **错误文案矩阵**
   - Cloud API unavailable：无法访问当前 server，检查网络、登录或 server 地址。
   - Desktop API starting/error/stopped：桌面内置 API 未就绪或启动失败，建议重试/重启桌面端。
   - Local Runtime stopped/error：本机执行器未启动或启动失败，建议检查 profile 后启动或重启 runtime。
   - Relay disconnected/reconnecting/error：本机执行器进程存在，但未连上 server relay，建议检查 server URL/网络/注册状态。
   - Registration missing/invalid：backend claim/registration 未完成或 token 已失效，建议重新登录桌面端或重新领取 runner registration token。
   - Active sessions block restart：当前有运行中会话，结束后再重启 runtime。

8. **UI 分区**
   - 顶部健康总览：四层状态 chips/dots，显示最严重层级与主要恢复动作。
   - 连接链路：Cloud API -> Desktop API -> Local Runtime/Runner -> Relay，逐层展示状态、target、last_error。
   - 注册与身份：backend_id、runner/backend name、registration source、machine/profile/scope/capability slot、last claimed/registered/seen。
   - 操作区：刷新、重启 runtime/runner、停止（若保留）、清空日志、复制日志。
   - 桌面设置：开机自启动、启动到托盘、启动后自动连接 runtime。
   - 日志区：level filter、tail、copy、clear；不把日志作为状态推断输入。

9. **与桌面/runner 的依赖**
   - Windows desktop 任务需提供 OS 自启动/托盘设置 command 与持久化位置。
   - Runner 任务需提供 registration source、relay connection、restart/logs capability 的稳定 DTO。
   - 若 runner restart/logs 不是桌面可管范围，UI 只展示“由 runner service 管理”，不要提供无效按钮。

10. **验证策略**
   - DTO/generated 更新后跑 `pnpm run contracts:check`。
   - 前端 view model/文案 mapper 单测覆盖四层状态组合、registration source、active-session restart error。
   - Rust 单测覆盖日志脱敏扩展与 logs clear/tail。
   - 桌面手工验收覆盖 Desktop API starting/error、runtime stopped/running/error、relay reconnect、日志复制不含 token。

### 3. 推荐的 implement 小步提交清单

1. **补齐 handoff contract 与计划 gate。**
   - 在 `design.md` 写清 runner/desktop 依赖字段。
   - 在 `implement.md` 第一节加“开始编码前确认 handoff”的检查项。

2. **后端/runner DTO 基础。**
   - 新增或接收 registration source 与 relay connection contract。
   - 更新 Rust contract crate 与 generated TS。
   - 验证 `pnpm run contracts:check`。

3. **Tauri/local runtime snapshot 扩展。**
   - 扩展 `LocalRuntimeStatus` 或新增 `runtime_diagnostics_snapshot`。
   - 增加 relay connection last_error/last_connected/retry_count 写入路径。
   - 保持 `LocalRuntimeClient` port 为 app-web 唯一依赖。

4. **日志脱敏增强。**
   - 扩展 Rust 脱敏函数。
   - 单测覆盖 query、Bearer、JSON field、大小写变体。
   - 确认 logs_tail/copy/export 都读脱敏后消息。

5. **桌面设置 commands。**
   - 增加 load/save settings DTO：`auto_launch`, `start_to_tray`, `auto_connect_runtime`。
   - Windows desktop handoff 未到时只实现已确认字段，避免假 UI。

6. **前端 service/model 层。**
   - 新增 diagnostics query/hooks/view model。
   - 明确 `RegistrationSource` 使用 generated union 或 Tauri port union。
   - 单测覆盖状态优先级、错误文案、恢复动作。

7. **设置页 UI 分区。**
   - 将现有 LocalRuntimeView 中状态/日志能力整合进新的诊断分区，或在其上方增加运行链路总览。
   - 使用 `Card/CardHeader/Badge/Notice/StatusDot/Button/Select/TextInput/CheckboxField`，避免新建 ad-hoc 控件。

8. **操作行为。**
   - 接入刷新、restart、logs clear/copy。
   - restart blocked 时显示面向用户的 Notice。
   - 独立 runner 无法由桌面重启时显示只读提示。

9. **验证与手工场景。**
   - `pnpm run contracts:check`
   - `pnpm run frontend:check`
   - `pnpm run frontend:lint`
   - Rust 相关单测或 `pnpm run backend:clippy`（若改 Rust 诊断/logging）
   - 桌面手工验收：API 启动、runtime 启停/重启、relay 断网重连、日志清空/复制、token 脱敏。

### 4. 需要从 runner 和 Windows desktop 任务接收的 handoff contract

**Runner handoff**

- Runner 注册方式枚举：`desktop_access_token | runner_registration_token`，由 server/runner DTO 明确返回。
- Runner claim/registration response 字段：`backend_id`, `name`, `profile_id`, `machine_id`, `machine_label`, `share_scope_kind`, `share_scope_id`, `capability_slot`, `registration_source`, `claimed_at`。
- Relay connection snapshot：`state`, `relay_ws_url_redacted`, `last_connected_at`, `last_disconnected_at`, `last_error`, `retry_count`, `next_retry_at`, `registered_backend_id`。
- Runtime process snapshot：`state`, `pid`（如适用）, `version`, `started_at`, `last_exit`, `last_error`, `managed_by`。
- Logs contract：tail command/API、clear command/API、limit 上限、日志条目字段、脱敏由 producer 保证还是由 desktop/web 二次保证。
- Restart contract：是否支持远程/桌面触发；active sessions 时错误 code；Windows service runner 的权限/失败文案。

**Windows desktop handoff**

- Settings DTO 与命令：`desktop_runtime_settings_load/save` 或等价接口。
- 字段语义：`auto_launch`（随系统登录启动 app）、`start_to_tray`（启动后隐藏主窗口/显示托盘）、`auto_connect_runtime`（桌面启动后按 profile 自动连接 local runtime）。
- 托盘行为：关闭窗口是退出还是最小化到托盘；托盘菜单是否含打开/退出/重启 runtime。
- OS 集成错误：权限不足、注册登录项失败、托盘不可用、设置持久化失败的稳定错误 code/message。
- 手工验收脚本：Windows 登录项开关、重启 app 后行为、托盘启动时是否仍能启动 Desktop API 与 auto-connect runtime。

## Caveats / Not Found

- `task.py current --source` 当前返回 none；本文件按用户显式给出的 `.trellis/tasks/06-26-runtime-diagnostics-settings` 写入。
- 未找到现有 `registration_source` 字段；当前代码无法可靠区分 desktop access token 与 runner registration token。
- 未找到 Windows login-item / tray startup 设置 command；当前只有 local runtime profile 的 `auto_start`。
- 未找到 local runtime 将 relay connection last_error/retry_count 写入 snapshot 的路径；目前 ws_client 主要写 `diag!`。
- 未改产品代码、规划文件或 spec；只新增本 research 文件。
