# 桌面本机 Runner 启动链路收束 - Implement Plan

## Phase 0 - Decision Locked

- [x] 桌面 runner 进程形态固定为 embedded local-owned runner host。
- [x] 本任务不做 sidecar `agentdash-local.exe` / externalBin / Windows Service 集成。
- [x] 实现顺序固定为：单实例 -> `agentdash-local` 内嵌 host -> Tauri/Web 启动桥接 -> settings/profile 语义 -> diagnostics/UI -> release validation。

## Phase 1 - Single Instance

- [x] 在 `agentdash-local-tauri` 加入单实例能力，优先评估 Tauri v2 single-instance 插件。
- [x] 第二实例启动时恢复/聚焦主窗口。
- [x] 确认第二实例不会初始化 Desktop API、runtime manager、runner supervisor。
- [x] 增加最小 Rust/desktop check，必要时补手工验收步骤。

## Phase 2 - `agentdash-local` Embedded Runner Host

- [x] 在 `agentdash-local` 抽出可嵌入的 desktop runner host / supervisor 状态模型。
- [x] Host 复用现有 claim、`LocalRuntimeConfig`、relay connection、status reporter、token redaction 逻辑。
- [x] Host 暴露清晰 API：`ensure_started(input)`、`stop(reason)`、`snapshot()`、`record_auth_state(...)` 或等价接口。
- [ ] Host 状态覆盖：`idle`、`disabled`、`waiting_for_auth`、`waiting_for_api`、`claiming`、`starting`、`running`、`retrying`、`error`、`stopping`、`stopped`。
- [x] Host 避免重复 claim、重复 profile 写入或重复 relay connect。

## Phase 3 - Tauri/Web Startup Bridge

- [x] Tauri `DesktopState` 持有 local-owned host，并在 setup 后初始化 host snapshot。
- [x] Tauri 通过 `ensure_started/stop/snapshot` 调用 local-owned host。
- [x] Web app 的 auto-connect effect 改为通知 token/请求 native ensure，移除“失败后永久跳过”的一次性标记。
- [x] 自动启动、托盘启动、设置页启动收束到同一 native service。
- [x] Dashboard API 未就绪、未登录、claim 失败、relay 失败时进入可诊断和可重试状态。

## Phase 4 - Settings/Profile Semantics

- [ ] 明确 `DesktopAppSettings.auto_connect_local_runtime` 与 `LocalRuntimeProfile.auto_start` 的关系。
- [ ] 让 profile 里的 server URL、workspace roots、executor_enabled 仍作为启动配置事实源。
- [ ] `external` API mode 只决定 Dashboard API origin，不关闭 desktop embedded runner host。

## Phase 5 - Diagnostics/UI

- [ ] 扩展 desktop runtime snapshot 或新增 desktop runner snapshot。
- [ ] 设置页展示 native supervisor state、last error、retry 状态和 manual retry。
- [ ] 保持 cloud diagnostics 中 `desktop_access_token` / `runner_registration_token` 的区分。
- [ ] 确保日志和 UI 不泄漏 token-bearing 字段。

## Phase 6 - Release Validation

- [x] `pnpm run desktop:check`
- [ ] 必要时 `pnpm run desktop:bundle`
- [ ] Windows 手工验收：
  - [ ] 全新安装后启动 UI。
  - [ ] 登录后自动拉起本机执行面。
  - [ ] 连续双击/重复启动只保留一个进程。
  - [ ] 关闭窗口到托盘后 runtime 保持在线。
  - [ ] 显式退出停止桌面托管 runner/runtime。
  - [ ] 云端不可达时进入可诊断状态，恢复后可重试上线。

## Review Gates

- 实现前 review：确认单实例库/方案、embedded local-owned host API、supervisor 状态机。
- 实现后 check：确认无重复进程、无重复 backend claim、无 token 泄漏、桌面/runner 语义没有重新漂移。

## Rollback Points

- 单实例改动可独立回滚。
- Supervisor 改动应保留旧 `runtime_start` command 的手动入口，方便问题定位。
- UI 诊断扩展不应改变云端 backend enrollment 逻辑。
