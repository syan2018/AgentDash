# Desktop local ownership cleanup

## Goal

实现 design backlog Slice 9 / D11：把桌面端本机 runtime 的 durable facts 下沉到
`agentdash-local`，让 `agentdash-local-tauri` 只保留 Tauri shell adapter 职责。当前 Tauri
`main.rs` 同时拥有 profile/settings DTO、profile/settings 文件 IO、desktop access-token ensure
client、claim response validation 和 `LocalRuntimeConfig` 投影，导致本机 runtime 的长期语义散落在
shell 层。

## Requirements

- `agentdash-local` owns desktop runtime durable facts:
  - `LocalRuntimeProfile` DTO、profile load/save/delete、normalization；
  - `DesktopAppSettings` DTO、settings load/save、normalization；
  - desktop access-token ensure request/response DTO、HTTP client、response validation；
  - profile/start request 到 `LocalRuntimeConfig` 的投影。
- `agentdash-local-tauri` 只保留 shell adapter：
  - Tauri command signature and `Result<_, String>` mapping；
  - tray/window/lifecycle/autostart OS adapter；
  - `DesktopRunnerHost` start/stop/restart orchestration。
- 清理旧问题优先于加法：不得只新增 `agentdash-local` API 后继续保留 Tauri 内部 profile/settings/claim
  实现作为活动路径。
- 保持当前用户语义：
  - profile load 不存在时返回 `None`；
  - profile save/load 会填充 machine identity、trim server origin、清空 persisted access token；
  - desktop settings 默认 `auto_connect_local_runtime = true`；
  - autostart save 仍由 Tauri OS adapter 执行，但 settings 文件写入可委托 local；
  - runtime start 可带一次性 access token，claim 成功后构建 `LocalRuntimeConfig`；
  - ensure response 必须校验 machine、scope、capability_slot、registration_source。
- 本项目预研期不保留兼容路径；如果旧 DTO 命名阻碍正确 owner，可以移动或重命名，但 frontend command
  payload shape 不应无意义 churn。
- Subagent 执行约束：实现 worker 不跑大规模 Rust 编译、full workspace check 或 broad suites；允许
  scoped `rg`、`cargo fmt`、小型定向 Rust tests。最终集成 check 统一决定更大范围编译。

## Acceptance Criteria

- [x] `crates/agentdash-local` 暴露 desktop profile/settings/claim/start-config API。
- [x] `crates/agentdash-local-tauri/src/main.rs` 不再定义 profile/settings/claim DTO 和 claim HTTP
  implementation；只调用 `agentdash-local` API。
- [x] `profile_load` / `profile_save` / `profile_delete` Tauri commands 委托 local library。
- [x] `desktop_settings_load` / `desktop_settings_save` 的 settings IO 委托 local library；autostart OS
  enable/disable 仍在 Tauri adapter。
- [x] `runtime_start` / auto-connect path 使用 local library 完成 start request normalization、desktop
  ensure claim 和 `LocalRuntimeConfig` projection。
- [x] Tests 覆盖 profile/settings roundtrip、claim response validation、claim request error mapping 和
  runtime config projection。
- [x] Static search 证明 Tauri main 中无活动的 profile/settings file IO、desktop ensure HTTP client、
  claim response validation 分叉。
- [x] Spec 更新记录 desktop shell 与 `agentdash-local` 的 owner 分工。

## Notes

- Source: `.trellis/tasks/06-30-design-backlog-review/design-review.md#d11-desktop-profile--claim--settings-into-agentdash-local`.
- This is Slice 9 from `.trellis/tasks/06-30-design-backlog-review/implementation-slices.md`.
- Completed shape:
  - `agentdash-local` owns `desktop_profile`, `desktop_settings` and `desktop_claim`.
  - `agentdash-local-tauri` keeps command/error mapping, autostart OS adapter, tray/window lifecycle
    and `DesktopRunnerHost` orchestration.
