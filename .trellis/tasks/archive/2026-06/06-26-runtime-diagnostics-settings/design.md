# 运行状态诊断与设置体验 - Design

## Goal And Non-Goals

目标是在设置页提供清晰的本机运行链路诊断视图，让用户能区分 Cloud API、Desktop API、Local Runtime/Runner、WebSocket relay 的状态、最近错误和恢复动作。

非目标：

- 不替代 session events、runtime health、Context Audit。
- 不从日志推断状态。
- 不把 Desktop API 与 Local Runner 混称为“本机 API”。
- 不在桌面设置中管理独立 runner Windows/Linux service，除非后续子任务明确交付 service 管理 bridge。

## State Sources

状态事实源：

- `cloud_api`：Web HTTP health/current user/settings 请求和 Project event stream lifecycle。
- `desktop_api`：Tauri `desktop_api_snapshot`，仅桌面宿主可用。
- `local_runtime`：Tauri `runtime_snapshot` 与 LocalRuntimeManager command。
- `runner`：云端 `/backends`、`/backends/runtime-summary` 与 runner handoff snapshot。
- `relay_connection`：runner/local runtime 结构化上报的 relay handshake/connected/reconnecting/error 状态。

UI 不从 `backend.online` 或日志文本反推 relay connection。`backend.online` 表示云端 relay registry 可见性，relay snapshot 表示本机连接过程诊断。

## Diagnostics Snapshot Contract

推荐聚合 DTO：

```ts
type RuntimeDiagnosticsSnapshot = {
  generated_at: string;
  cloud_api: ApiLayerStatus;
  desktop_api: DesktopApiLayerStatus | null;
  local_runtime: LocalRuntimeLayerStatus | null;
  runner: RunnerLayerStatus | null;
  relay_connection: RelayConnectionStatus | null;
  registration: RuntimeRegistrationStatus | null;
  logs: LocalLogEvent[];
  settings: DesktopRuntimeSettings | null;
};
```

状态枚举：

- `LayerState = unknown | checking | healthy | degraded | unavailable | disabled`
- `RelayConnectionState = not_configured | connecting | registered | reconnecting | disconnected | error`
- `RegistrationSource = desktop_access_token | runner_registration_token`

跨后端 DTO 使用 generated contracts；Tauri-only DTO 放在 `@agentdash/core/local-runtime` port。Feature view model 显式转换，不让 UI 直接猜 raw DTO。

## Registration Source

Desktop access token 来源：

- 用户登录 token 调 `/api/local-runtime/ensure`。
- UI 展示“桌面登录授权”。
- 不展示 token。

Runner registration token 来源：

- runner 使用 registration token 领取/注册 backend。
- UI 展示“Runner 注册令牌”。
- 不展示 token。

`registration` 字段至少包含：

- `source`
- `backend_id`
- `profile_id`
- `machine_id`
- `machine_label`
- `share_scope_kind`
- `share_scope_id`
- `capability_slot`
- `claimed_at`
- `registered_at`
- `last_seen_at`

来源必须由 server/runner 明确返回，不从 endpoint、scope 或 backend type 推断。

## Logs And Redaction

所有 local runtime / runner logs、copy/export、recent error 都经过同一脱敏函数。

脱敏覆盖：

- URL query: `token/access_token/refresh_token/auth_token/relay_token/registration_token`
- Bearer header
- JSON/string field
- 大小写变体

日志只表达平台过程诊断。Session events、shell output、Context Audit 不进入本日志区。

UI 支持：

- 刷新 tail。
- 复制脱敏后的日志。
- 清空日志。
- level filter if cheap。

## Commands

已有命令：

- `runtime_snapshot`
- `runtime_start`
- `runtime_stop`
- `runtime_restart`
- `logs_tail`
- `logs_clear`
- `desktop_api_snapshot`

需要补齐或确认：

- `desktop_runtime_settings_load/save`
- optional `runtime_diagnostics_snapshot`
- `runner_restart` 是否属于桌面可管范围

`runtime_restart` 遇到 active session/canceling session 时，UI 必须显示可理解错误：“当前有会话正在运行，结束后再重启 runtime”。

## Error Copy Matrix

- Cloud API unavailable：无法访问当前 server，检查网络、登录或 server 地址。
- Desktop API starting/error/stopped：桌面内置 API 未就绪或启动失败，建议重启桌面端。
- Local Runtime stopped/error：本机执行器未启动或启动失败，检查 profile 后启动或重启。
- Relay disconnected/reconnecting/error：本机执行器进程存在，但未连上 server relay，检查 server URL、网络或注册状态。
- Registration missing/invalid：backend claim/registration 未完成或 token 已失效，重新登录桌面端或重新领取 runner token。
- Active sessions block restart：当前有运行中会话，结束后再重启。

## UI Structure

设置页新增本机运行诊断区域：

- 顶部健康总览：四层状态 chips/status dots，显示最严重层级和主要恢复动作。
- 连接链路：Cloud API -> Desktop API -> Local Runtime/Runner -> Relay。
- 注册与身份：backend id、name、registration source、machine/profile/scope/capability slot、last claimed/registered/seen。
- 操作区：刷新、重启 runtime/runner、停止、清空日志、复制日志。
- 桌面设置：开机自启动、启动到托盘、启动后自动连接 runtime。
- 日志区：tail、copy、clear、filter。

Use existing primitives/tokens；避免新增 ad-hoc 圆角/字面色控件。

## Handoff Dependencies

From `local-runner-daemon`：

- `status --json` schema。
- relay connection state。
- registration source。
- log tail path/format。
- restart/service management boundary。

From `windows-desktop-installer-background`：

- desktop settings DTO。
- autostart command。
- tray/window lifecycle behavior。
- active execution exit-blocking summary if available。

To `distribution-release-validation`：

- diagnostics UI paths。
- logs copy/export脱敏 evidence。
- restart/recovery steps。
- manual checks for each layer。
