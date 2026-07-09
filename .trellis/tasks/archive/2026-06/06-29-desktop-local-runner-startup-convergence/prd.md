# 桌面本机 Runner 启动链路收束

## Goal

收束 Windows 桌面壳自包含 local/runner 执行能力的启动生命周期，确保桌面安装包启动后会可靠拉起同一套本机执行面，并且重复双击、登录自启动、托盘恢复等入口不会产生多个桌面壳进程或多个等价 runner/runtime 连接。

本任务承接既有“本机运行形态产品化”和“enrollment 路径收束”模型：桌面壳内自包含的 local 执行能力与 runner 在执行语义上等价，都应作为 Local Execution Backend 连接云端；差异主要在授权入口和生命周期宿主，而不是执行能力本身。

## User Value

- 桌面用户打开 AgentDash 后，本机执行面自动可用，不需要进入设置页手动补救。
- 多次启动桌面 App 只会恢复已有窗口和托盘状态，不会制造多个后台执行进程或多个 relay 连接。
- 诊断页能清楚解释桌面自包含 local/runner 是否已启动、是否在连接、失败原因是什么，以及用户可以如何恢复。
- 发布验收能覆盖“全新安装 -> 登录 -> 本机执行面在线 -> 关闭到托盘 -> 再次启动 -> 仍是单实例”的完整路径。

## Confirmed Facts

- 产品化父任务要求 Windows 桌面完整安装包包含 Tauri 桌面壳、内置前端、Desktop API、Local Runtime 生命周期管理、托盘、后台运行、自启动设置；见 [`.trellis/tasks/06-26-local-runtime-distribution/prd.md`](../06-26-local-runtime-distribution/prd.md)。
- enrollment 收束任务已明确 Desktop 与 Standalone Runner 共享本机执行面概念模型，但保留不同授权入口；Desktop 使用用户 access token 调 `/api/local-runtime/ensure`，Runner 使用 registration token 调 `/api/local-runtime/runner/claim`；见 [`.trellis/tasks/06-27-local-backend-enrollment-convergence/prd.md`](../06-27-local-backend-enrollment-convergence/prd.md)。
- `crates/agentdash-local-tauri/Cargo.toml:25` 当前只启用 `tauri` 的 `tray-icon` feature，没有单实例插件或等价进程互斥。
- `crates/agentdash-local-tauri/src/main.rs:870` 当前 Tauri builder 直接启动主进程并管理 `DesktopState`，没有注册 single-instance 行为。
- `crates/agentdash-local-tauri/src/main.rs:889` 的 `.setup(...)` 会配置托盘、启动/复用 Dashboard API、设置窗口显示，但不会从 profile 主动启动本机执行面。
- `crates/agentdash-local-tauri/src/main.rs:1001` 的 `start_runtime_from_profile` 只从托盘菜单“启动本机 runtime”触发。
- `packages/app-web/src/App.tsx:218` 当前桌面自动启动由 Web app 在 `currentUser` 就绪后调用 `ensureDesktopLocalRuntimeStarted(...)`。
- `packages/app-web/src/desktop/localRuntimeBridge.ts:55` 当前 `desktopRuntimeAutoConnectAttempted` 在真正启动成功前就置为 `true`，失败后同一页面生命周期内不会自动重试。
- `scripts/desktop-build.js:14` 默认桌面构建使用 `external` API mode；安装包默认连接远端 server，不通过本地端口冲突自然阻止多开。
- 当前本机检查未发现 `AgentDashLocalRunner` Windows Service，也未发现 `C:\ProgramData\AgentDash\runner\config.toml`，说明桌面包当前没有依赖已安装的独立 runner service 才能工作。

## Requirements

- R1. 桌面安装包必须明确拥有一个自包含的 local/runner 启动宿主。该宿主负责在桌面进程生命周期内启动、停止、重启和报告本机执行面状态，而不是依赖 Web app 页面 effect 的偶发触发。
- R2. 桌面 local/runner 启动应复用现有 enrollment 模型：用户已登录时使用 desktop access token 调 `/api/local-runtime/ensure`，获得 `backend_id + relay_ws_url + auth_token` 后连接 relay。
- R3. 桌面壳必须提供跨进程单实例保护。第二个桌面实例启动时应唤醒/聚焦已有主窗口，并退出新实例，不能创建第二个 execution backend lifecycle owner。
- R4. 自动启动应具备可重试语义。Dashboard API 未就绪、用户登录态稍晚到达、profile 写入失败、ensure 请求失败、relay 暂时不可达时，状态应可诊断，且不因一次失败永久跳过。
- R5. `auto_connect_local_runtime`、`profile.auto_start`、托盘“启动本机 runtime”三条入口的语义要收束：它们都应调用同一个 desktop-native 启动服务，不应分别实现或分别绕过状态检查。
- R6. Desktop API mode 与 local/runner 启动语义要明确解耦。`external` 只表示 Dashboard API 指向远端 server，不表示桌面包不自带本机执行面。
- R7. 桌面设置/诊断 UI 应展示桌面自包含 local/runner 的真实状态，包括未配置、等待登录、领取凭据中、relay 连接中、运行中、失败、停止中。
- R8. 日志和状态输出必须继续脱敏 access token、registration token、relay auth token。
- R9. 本任务不重新设计 runner registration token、多 project grant、backend stable id、ProjectBackendAccess 授权模型；这些以前置任务为事实源。

## Decisions

- D1. 首版采用 **embedded local-owned runner host**。runner 状态机、claim、relay connection、status/log redaction 由 `agentdash-local` 拥有；Tauri 通过 library API 调用它，runner 仍运行在 `agentdash-local-tauri.exe` 进程内。
- D2. 本任务不把桌面 runner 改为独立 `agentdash-local.exe` sidecar。sidecar 进程模型保留为后续增强，待 embedded host 的状态契约稳定后再评估。
- D3. Tauri 是桌面生命周期宿主，负责单实例、托盘、窗口、显式退出和调用 local-owned runner host；Web app 不再直接拥有 runner auto-start 状态机，只负责通知登录态/token availability 和展示 snapshot。
- D4. 单实例保护是本任务的第一实现切片。原因是没有进程级单实例时，后续任何 auto-start 修复都仍可能被重复桌面进程绕过。

## Acceptance Criteria

- [ ] Windows 桌面 App 启动后，在用户已登录且 `auto_connect_local_runtime=true` 时，会由 Tauri/native lifecycle 可靠拉起桌面自包含 local/runner，并最终在云端显示对应 backend 在线。
- [ ] 全新启动时如果 Dashboard API 或登录态尚未就绪，local/runner 启动进入等待/重试状态；条件满足后自动继续，不需要用户打开设置页点击启动。
- [ ] `runtime_start`、托盘启动、自动启动、profile auto-start 都走同一启动服务；重复调用只复用正在启动/运行的同一个 runtime，不会重复 claim 或重复连接。
- [ ] 桌面壳具备单实例保护：连续双击安装包快捷方式或登录自启动与用户手动启动重叠时，只保留一个桌面壳进程；第二次启动会恢复/聚焦主窗口。
- [ ] 多开防护覆盖 `external` API mode；不能依赖 Desktop API 端口冲突作为单实例机制。
- [ ] 自动启动失败后，状态与日志能说明失败阶段；临时失败支持重试，用户也可通过托盘/设置页重新触发同一启动链路。
- [ ] 关闭窗口到托盘不会停止本机执行面；显式退出会按设计停止桌面自包含 local/runner 并清理 sidecar/child handles。
- [ ] 发布验收文档覆盖全新安装、首次登录自动启动、重复启动、关闭到托盘、显式退出、断网重连、远端 server 不可达、卸载/重装后的行为。
- [ ] 测试或手工验收能证明桌面自包含 local/runner 与独立 `agentdash-local run/setup/service` 在 relay credential、runner status、executor capability 语义上等价；差异仅体现在授权入口和生命周期宿主。
- [ ] token-bearing 字段在 Rust 日志、前端 console、诊断 snapshot、status/logs UI 中保持脱敏。
