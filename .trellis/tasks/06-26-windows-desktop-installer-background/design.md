# Windows 桌面安装包与后台运行 - Design

## Goal And Scope

本子任务只覆盖 Windows 桌面完整安装包、Tauri shell 生命周期、系统托盘、关闭到后台、Windows 登录自启动和桌面 runtime 自动连接。不覆盖独立 Local Runner 的 Windows Service；那属于 `local-runner-daemon`。

Windows 桌面完整安装包包含 Tauri 壳、内置前端、Desktop API 与 Local Runtime 管理能力。Desktop API 是桌面内置前端的 Dashboard API 宿主，默认绑定 `127.0.0.1`，不代表 Local Runner 的通信模型。

## Artifacts And Process Model

产物边界：

- setup exe：NSIS installer，用户交付对象，负责安装/升级/卸载、开始菜单/桌面入口、卸载注册信息、安装器元数据。
- app exe：安装后的 `AgentDash.exe`，是真正运行 Desktop API、Web Dashboard、LocalRuntimeManager、托盘和后台驻留的进程。

`pnpm run desktop:build` 生成可运行 app 构建，不代表正式安装流程。`pnpm run desktop:bundle` 产出 NSIS setup exe，是 release validation 的验收对象。

## Desktop API Contract

- release bundle 默认使用 builtin Desktop API。
- release bundle 中 Desktop API 必须绑定 loopback：`127.0.0.1:3001`。
- `DesktopApiSnapshot.state` 保持 `starting | running | error | stopped`。
- DashboardHost 继续等待 `desktop_api_snapshot` running 和 `/api/health` ready 后渲染 Web Dashboard。
- `external` / `sidecar` mode 只作为开发/诊断入口；如果 release 支持 sidecar，则必须校验 origin host 为 loopback。

## Window And Tray Lifecycle

窗口 label 继续使用 `main`。

默认行为：

- 点击自绘标题栏关闭按钮或系统 close request：阻止默认退出，隐藏主窗口，进程继续运行。
- 托盘左键或菜单 `打开 AgentDash`：show/unminimize/focus 主窗口。
- 托盘菜单 `退出 AgentDash`：设置 explicit quit flag 后允许真实退出。

托盘菜单：

- 打开 AgentDash。
- 启动本机 runtime。
- 停止本机 runtime。
- 查看状态。
- 退出 AgentDash。

托盘 runtime 菜单根据 `runtime_snapshot` enable/disable。无 profile 时显示明确状态，不触发隐式配置。

## Explicit Exit And Runtime Semantics

隐藏窗口不停止 Desktop API 或 Local Runtime。显式退出会终止桌面进程，因此也会终止同进程 Local Runtime。

若当前 runtime 存在 active execution/lease，退出必须有明确策略。第一版保守策略：

- 如果能获得 active execution summary，则阻止直接退出并提示用户先等待/停止任务。
- 如果当前状态接口还没有 active count，则显式退出只基于 runtime state 警告，具体 active execution summary 作为 `runtime-diagnostics-settings` handoff。

## Desktop Settings Model

新增桌面偏好，不复用 runtime profile `auto_start` 的多重语义：

- `launch_at_login`：Windows 登录后启动 AgentDash。
- `start_minimized_to_tray`：启动后隐藏主窗口，仅显示托盘。
- `auto_connect_local_runtime`：桌面启动并完成用户认证后自动连接本机 runtime。

Runtime profile 保存 server/profile/workspace/executor/backend claim 事实。Desktop App settings 保存窗口/托盘/登录启动偏好。

## Autostart

Windows 登录自启动是 App 级能力。实现可以选择：

- Rust command 包装 Windows startup/autostart 操作，最小化前端权限。
- 或 Tauri autostart plugin，但必须补 capability/permission 并限制 exposed API。

安装器不默认开启用户自启动。设置页启用 `launch_at_login` 后写入 startup entry；卸载清理 AgentDash 管理的 startup entry。

`start_minimized_to_tray=true` 时，Tauri setup 应尽早隐藏主窗口，避免明显闪现，同时仍启动 Desktop API 和必要 runtime auto-connect 流程。

## Runtime Auto-Connect Ownership

当前代码存在多条自动启动路径。第一版收敛为单一 owner：

- 推荐由 Web `AuthGate` 在 current user ready 且 Desktop API health ready 后触发一次 `ensureDesktopLocalRuntimeStarted()`，因为它能拿到当前 access token。
- Rust setup 只启动 Desktop API、加载设置/profile，不主动 claim runtime。
- `LocalRuntimeView` mount 不再重复 auto start；设置页只展示状态和用户操作。

`runtime_start` 或 bridge 层必须对 `starting/running` 幂等，避免重复 ensure/claim。

## Frontend / Tauri Surface

`packages/app-tauri/src/runtimeApi.ts` 继续适配 `LocalRuntimeClient`。Desktop App settings/window lifecycle API 单独命名，避免普通 Web 获取桌面能力：

- `desktopSettingsLoad`
- `desktopSettingsSave`
- `desktopAutostartSetEnabled`
- `desktopAutostartIsEnabled`
- `desktopQuitRequest`

普通 Web 不显示桌面 settings entry。

## Security And Permissions

- Desktop API release build 只绑定 loopback；localhost 不是认证边界，但不能暴露到 LAN。
- Tauri capabilities 最小授权：窗口 show/hide/focus/close、自绘标题栏、托盘、必要的 autostart 权限。
- 外部链接继续只允许 `http/https`。
- CSP 当前为 `null`；本任务至少记录是否维持现状以及原因。如果收紧 CSP，需要验证 Web Dashboard bundle 资源加载。

## Installer / Uninstall Boundary

NSIS setup 负责：

- app exe 和资源安装。
- icons。
- start menu / desktop shortcut。
- uninstall info。
- installer metadata。

Uninstall 负责：

- 删除安装目录。
- 删除 shortcuts。
- 删除 uninstall info。
- 删除 AgentDash 管理的 startup entry。

Uninstall 不删除 local-runtime data root、machine identity、profile、MCP config、logs/cache，除非未来增加显式“删除用户数据”选项。

## Handoff

To `runtime-diagnostics-settings`：

- desktop settings DTO。
- tray/window lifecycle command。
- Desktop API status。
- runtime auto-connect owner。
- active execution exit-blocking dependency。

To `distribution-release-validation`：

- `pnpm run desktop:bundle` output contract。
- setup exe vs app exe distinction。
- installer metadata。
- startup entry cleanup behavior。
- manual acceptance steps。
