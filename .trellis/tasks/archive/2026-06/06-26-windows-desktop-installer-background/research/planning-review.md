# Research: Windows desktop installer and background runtime planning review

- Query: 为子任务 `Windows 桌面安装包与后台运行` 审阅现有 PRD/design/implement，补齐安装包产物、托盘、窗口生命周期、自启动、启动到托盘、runtime 自动连接、Desktop API localhost、安全/权限、卸载边界以及与相邻任务的 handoff contract。
- Scope: mixed
- Date: 2026-06-26

## Findings

### Files Found

- `.trellis/tasks/06-26-windows-desktop-installer-background/prd.md` - 已定义 Windows 完整安装包、后台运行、托盘、自启动、启动到托盘、runtime 自动连接和 localhost Desktop API 的验收方向。
- `.trellis/tasks/06-26-windows-desktop-installer-background/design.md` - 目前只有高层架构、托盘菜单、设置和打包说明，缺少进程/产物/权限/卸载/跨任务接口合同。
- `.trellis/tasks/06-26-windows-desktop-installer-background/implement.md` - 目前是粗粒度 checklist，缺少按可验证小步拆分的实现顺序和每步验证点。
- `.trellis/tasks/06-26-windows-desktop-installer-background/implement.jsonl` - 已登记 desktop-local-runtime、frontend design language 和 backend logging 相关 context。
- `.trellis/tasks/06-26-windows-desktop-installer-background/check.jsonl` - 已登记 frontend quality 和 cross-layer thinking context。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - 桌面壳、Desktop API、Local Runtime profile/command/HTTP 边界的主要跨层规范。
- `.trellis/spec/frontend/design-language.md` - 桌面设置 UI 需要遵守的 token、primitive、surface 和 radius 约束。
- `crates/agentdash-local-tauri/tauri.conf.json` - Tauri 壳配置，当前有产品名、版本、identifier、窗口尺寸、无边框窗口和 bundle active。
- `crates/agentdash-local-tauri/src/main.rs` - Tauri 主进程，当前持有 Desktop API manager、LocalRuntimeManager、profile/runtime/MCP/log commands 和 API 启动逻辑。
- `crates/agentdash-local-tauri/capabilities/default.json` - 当前仅授予自绘标题栏所需窗口控制权限。
- `crates/agentdash-local-tauri/Cargo.toml` - 当前 Tauri dependency 未引入 autostart 插件，`tauri` features 为空。
- `packages/app-tauri/src/App.tsx` - 桌面前端宿主，注入 LocalRuntimeClient、目录浏览、外部链接打开桥，等待 Desktop API health 后渲染 Web Dashboard。
- `packages/app-tauri/src/DesktopTitlebar.tsx` - 自绘标题栏，关闭按钮当前直接调用 `getCurrentWindow().close()`。
- `packages/app-tauri/src/runtimeApi.ts` - Tauri `invoke()` 到 `LocalRuntimeClient` 的适配层，当前不含 desktop settings/autostart/window lifecycle API。
- `packages/app-web/src/desktop/localRuntimeBridge.ts` - Web Dashboard 内部的桌面 runtime 自动启动桥，认证完成后按 profile `auto_start` 调用 `runtimeStart()`。
- `packages/views/src/local-runtime/LocalRuntimeView.tsx` - 本机 runtime 设置/诊断面板，当前也会在加载到 `profile.auto_start` 后自动启动 runtime。
- `scripts/desktop-build.js` - 桌面构建入口，默认 `builtin` API mode 和 `http://127.0.0.1:17301`。
- `scripts/lib/desktop-build.js` - 桌面构建参数解析、API mode/origin sidecar 注入、sccache 配置和 `pnpm exec tauri build` 调用。
- `package.json` - 定义 `desktop:check`、`desktop:build`、`desktop:bundle`，其中 bundle 使用 `--bundles nsis --no-sign --ci`。

### Current Plan Gaps / Risks

P0 - `auto_start` 语义已经混在一起，设计需要先拆清楚。`LocalRuntimeProfile` 当前只有 `auto_start` 字段（`crates/agentdash-local-tauri/src/main.rs:114` 到 `crates/agentdash-local-tauri/src/main.rs:116`；`packages/core/src/local-runtime/index.ts:32` 到 `packages/core/src/local-runtime/index.ts:36`），它表达的是 runtime 自动连接，不是 Windows 开机启动 AgentDash。PRD 需要的设置至少是三个独立偏好：开机启动 App、启动后进入托盘、启动后自动连接 runtime。

P0 - runtime 自动连接现在有重复触发路径。Rust setup 会调用 `auto_start_profile()`（`crates/agentdash-local-tauri/src/main.rs:645` 到 `crates/agentdash-local-tauri/src/main.rs:648`），Web `AuthGate` 在用户就绪后调用 `ensureDesktopLocalRuntimeStarted()`（`packages/app-web/src/App.tsx:215` 到 `packages/app-web/src/App.tsx:222`），`LocalRuntimeView` 加载 profile 时也会自动 `runtimeStart()`（`packages/views/src/local-runtime/LocalRuntimeView.tsx:79` 到 `packages/views/src/local-runtime/LocalRuntimeView.tsx:90`）。Design 必须指定单一 owner 和幂等行为，否则会重复 ensure/claim 或出现启动顺序竞争。

P0 - 托盘和窗口生命周期尚未落地。当前 `main.rs` 没有 `TrayIconBuilder`、menu、window close interception 或 explicit quit flag；Tauri run loop 只在 `RunEvent::Exit | ExitRequested` 时停止 API sidecar（`crates/agentdash-local-tauri/src/main.rs:670` 到 `crates/agentdash-local-tauri/src/main.rs:673`）。前端关闭按钮仍直接 close window（`packages/app-tauri/src/DesktopTitlebar.tsx:100` 到 `packages/app-tauri/src/DesktopTitlebar.tsx:104`）。如果不补拦截，点击关闭会退出进程而不是隐藏到托盘。

P0 - 显式退出与运行中任务的语义不够具体。Local runtime manager 在 Tauri 进程内持有（`crates/agentdash-local-tauri/src/main.rs:25` 到 `crates/agentdash-local-tauri/src/main.rs:29`），关闭窗口隐藏时 runtime 会继续运行，但显式退出桌面进程会终止同进程 runtime。现有 `LocalRuntimeStatus` 没有 active session/lease count（`packages/core/src/local-runtime/index.ts:3` 到 `packages/core/src/local-runtime/index.ts:11`），计划需要定义是否在运行中任务存在时拒绝退出、弹确认，还是允许退出并明确中断。

P0 - Desktop API 的 localhost 约束需要写成 production contract。内置 API 使用 `ApiServerOptions::desktop_localhost(DESKTOP_API_PORT)`（`crates/agentdash-local-tauri/src/main.rs:698` 到 `crates/agentdash-local-tauri/src/main.rs:706`），默认 origin 是 `http://127.0.0.1:17301`（`crates/agentdash-local-tauri/src/main.rs:956` 到 `crates/agentdash-local-tauri/src/main.rs:958`）。Desktop API 端口独立于普通 cloud/backend dev server 的 `3001`，原因是桌面安装包应避开常见本机调试端口，而普通 Web 开发入口仍保留原 dev server 约定。`desktop_api_config()` 接受 env/build default origin（`crates/agentdash-local-tauri/src/main.rs:993` 到 `crates/agentdash-local-tauri/src/main.rs:1033`），sidecar 还会把 origin host 写入 `HOST`（`crates/agentdash-local-tauri/src/main.rs:778` 到 `crates/agentdash-local-tauri/src/main.rs:788`）。如果 PRD 要求 Desktop API 只绑定 `127.0.0.1`，设计要规定 release bundle 仅使用 builtin localhost 或校验 host 为 loopback。

P1 - 安装包产物和 app exe vs setup exe 缺少边界。当前 `tauri.conf.json` 只写了 `bundle.active=true` 和 `targets="all"`（`crates/agentdash-local-tauri/tauri.conf.json:27` 到 `crates/agentdash-local-tauri/tauri.conf.json:33`），而脚本 `desktop:bundle` 通过 `--bundles nsis` 限定 NSIS（`package.json:24` 到 `package.json:25`）。Design 应明确：setup exe 是安装器产物，安装后 app exe 是真正运行并驻留托盘的进程；release validation 验证两者而不是把 setup exe 当应用进程。

P1 - 自启动实现路径没有写清楚。当前 `Cargo.toml` 未引入 `tauri-plugin-autostart`（`crates/agentdash-local-tauri/Cargo.toml:25` 到 `crates/agentdash-local-tauri/Cargo.toml:28`），capabilities 也只有窗口权限（`crates/agentdash-local-tauri/capabilities/default.json:6` 到 `crates/agentdash-local-tauri/capabilities/default.json:14`）。如果用 Tauri autostart 插件的 JS API，需要补 permissions；如果只通过 Rust command 包装，则 design 要记录权限最小化策略。

P1 - 启动到托盘需要避免窗口闪现。当前 `tauri.conf.json` 主窗口默认可见，且 `App.tsx` 第一屏始终渲染自绘标题栏和 DashboardHost（`crates/agentdash-local-tauri/tauri.conf.json:12` 到 `crates/agentdash-local-tauri/tauri.conf.json:21`；`packages/app-tauri/src/App.tsx:42` 到 `packages/app-tauri/src/App.tsx:48`）。如果 preference 是启动后进入托盘，Tauri setup 应尽早 hide 主窗口，并仍启动 Desktop API/runtime。

P1 - 安全/权限章节需要覆盖 CSP、capabilities、外部链接和本机目录浏览。当前 Tauri security CSP 是 `null`（`crates/agentdash-local-tauri/tauri.conf.json:23` 到 `crates/agentdash-local-tauri/tauri.conf.json:25`）；外部链接 command 已限制 `http/https`（`crates/agentdash-local-tauri/src/main.rs:311` 到 `crates/agentdash-local-tauri/src/main.rs:319`）；目录浏览 command 允许桌面 setup 选择器全盘浏览（`.trellis/spec/cross-layer/desktop-local-runtime.md` 的 Profile 章节说明本机目录浏览是 setup 选择器能力）。

P1 - 卸载边界需要从“安装器负责全部清理”改成“清理已知 AgentDash 管理项”。自启动一般是用户设置或 Tauri plugin 写出的 per-user 启动项；安装器能清理安装期 shortcut/uninstall info，也应清理已知 app autostart entry，但不应清用户 data root、profile、machine identity、MCP config，除非产品另设删除用户数据路径。

P2 - 现有设置 UI 用 `rounded-full` scope tab 和字面样式（`packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:81` 到 `packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:96`），与 `frontend/design-language.md` 的 token/radius 约束有偏差。若本任务触达桌面设置 UI，建议一并要求使用已有 `CheckboxField`/`Field`/`Button`/`Badge` primitive。

### Recommended Design Structure

建议把 `design.md` 扩成以下结构：

1. **Goal And Scope**
   - Windows-only 完整桌面安装包；不扩 macOS/Linux 生命周期。
   - 本任务拥有 Tauri shell 生命周期、Windows installer/autostart、托盘和 release bundle metadata；不拥有独立 runner Windows Service 语义。

2. **Artifacts And Process Model**
   - `setup exe`: NSIS 安装器，负责安装/升级/卸载 App、创建快捷方式、写入卸载信息、产出 release 可分发文件。
   - `app exe`: 安装后的 `AgentDash.exe`，是真正运行 Desktop API、内置 Web Dashboard、LocalRuntimeManager、托盘和后台驻留的进程。
   - `pnpm run desktop:build`: 构建 app，不产出安装包；`pnpm run desktop:bundle`: 产出 Windows NSIS setup exe。
   - bundle 输出路径由 Tauri CLI 决定，release validation 只读取构建脚本打印/约定的产物路径，不猜 glob。

3. **Desktop API Contract**
   - release bundle 默认且只使用 builtin Desktop API：`127.0.0.1:17301`。
   - `DesktopApiSnapshot.state` 保持 `starting | running | error | stopped`。
   - `DashboardHost` 继续等 `/api/health` ready 后渲染 Web Dashboard。
   - `external/sidecar` mode 只保留开发/诊断入口；生产构建校验 origin host 必须为 loopback，或 release script 固定 builtin。

4. **Window And Tray Lifecycle**
   - 主窗口 label 仍为 `main`。
   - 点击标题栏关闭或系统 close request：阻止默认退出，隐藏主窗口，进程继续运行。
   - 托盘左键或菜单 `打开 AgentDash`：show/unminimize/focus 主窗口。
   - 托盘菜单包含：打开 AgentDash、启动本机 runtime、停止本机 runtime、查看状态、退出 AgentDash。
   - 托盘状态项展示 Desktop API state + runtime state；启动/停止菜单根据 runtime state enable/disable。
   - 引入 explicit quit flag；只有托盘 `退出 AgentDash` 或受控 command 设置 flag 后才允许 process exit。

5. **Explicit Exit And Runtime Semantics**
   - 隐藏窗口不停止 Desktop API 或 Local Runtime。
   - 显式退出会停止 Desktop API sidecar（若 sidecar mode）并终止同进程 LocalRuntimeManager。
   - 若 runtime 有 active execution/lease，退出路径必须有明确语义：推荐拒绝直接退出并要求用户先停止/等待；如果当前 snapshot 没有 active count，则 handoff 给 `runtime-diagnostics-settings` 暴露 active execution summary 后再启用强退出确认。

6. **Desktop Settings Model**
   - 新增桌面偏好类型，例如 `DesktopAppSettings`:
     - `launch_at_login: boolean`
     - `start_minimized_to_tray: boolean`
     - `auto_connect_local_runtime: boolean`
   - runtime profile 继续保存 server/profile/workspace/executor/backend claim 事实；桌面 App 偏好保存到独立 desktop settings 文件或明确拆分后的 profile 字段。
   - `auto_start` 不再同时表达 Windows app autostart 和 runtime auto-connect。

7. **Autostart**
   - Windows 开机启动 AgentDash 是 App 级 autostart，由 Tauri/Rust command 或 autostart plugin 管理。
   - `launch_at_login=true` 时启用开机启动；`false` 时禁用。
   - autostart 启动后的窗口可见性由 `start_minimized_to_tray` 决定。
   - 安装器不默认开启用户自启动；卸载清理 AgentDash 管理的 startup entry。

8. **Runtime Auto-Connect**
   - 单一 owner：推荐 Web `AuthGate` 在 current user ready 且 Desktop API health ready 后触发一次 `ensureDesktopLocalRuntimeStarted()`，因为它能拿到当前 access token。
   - Rust setup 只加载 settings/profile 和启动 Desktop API，不主动 claim runtime，除非设计改为无 token personal mode 的 Tauri owner。
   - `LocalRuntimeView` 不应在 mount 时重复自动 start；设置页只展示状态和用户手动操作。
   - `runtime_start` 或桥函数必须对 `starting/running` 幂等。

9. **Frontend/Desktop API Surface**
   - `packages/app-tauri/src/runtimeApi.ts` 继续只适配 `LocalRuntimeClient`；新增 desktop app settings/window lifecycle API 时单独命名，例如 `desktopSettingsLoad/Save`, `desktopAutostartSetEnabled`, `desktopQuitRequest`。
   - `packages/app-web` 通过 desktop bridge 检测 Tauri 能力；普通 Web 不出现桌面设置入口。

10. **Security And Permissions**
    - Desktop API release build 只绑定 loopback；localhost 不是认证边界，仍要避免暴露到 LAN。
    - Tauri capabilities 最小授权：窗口 show/hide/focus/close、自绘标题栏、如使用 JS autostart API 则补 `autostart:allow-enable/disable/is-enabled`；若封装 Rust command，则不把 autostart JS permission 暴露给前端。
    - 外部链接仍只允许 `http/https`。
    - CSP 当前为 `null`，本任务至少要记录是否维持现状以及原因；若收紧 CSP，要和 Web Dashboard bundle 资源加载一起验证。

11. **Installer / Uninstall Boundary**
    - NSIS setup 安装 app exe、icons、shortcuts、uninstall info。
    - 卸载删除安装目录、shortcuts、uninstall info、AgentDash 管理的 startup entry。
    - 卸载不删除 `local-runtime` data root、machine identity、profile、MCP config、logs/cache，除非后续产品设计增加“删除用户数据”选择。

12. **Validation Matrix**
    - 构建：`pnpm run desktop:check`, `pnpm run desktop:bundle`。
    - Windows 手工：全新安装、启动 UI、关闭到托盘、托盘恢复、显式退出、开机启动、启动到托盘、自动连接 runtime、卸载 startup 清理、Desktop API loopback。
    - 负例：端口占用进入 `DesktopApiSnapshot.error`；非 loopback release origin 被阻止或不可配置。

### Recommended Implement Small-Step Commit List

1. `chore(desktop): 补齐 Tauri 托盘与自启动依赖边界`
   - 增加所需 Tauri/Rust dependency、features/capabilities。
   - 只做可编译空接线，不改业务行为。
   - 验证：`cargo check -p agentdash-local-tauri`。

2. `feat(desktop): 增加桌面应用偏好模型`
   - 新增 `DesktopAppSettings` load/save command。
   - 拆分 `launch_at_login`、`start_minimized_to_tray`、`auto_connect_local_runtime`。
   - 同步 TS desktop bridge 类型。
   - 验证：`pnpm --filter app-tauri typecheck`, `cargo check -p agentdash-local-tauri`。

3. `feat(desktop): 接入系统托盘菜单与窗口恢复`
   - 创建 tray icon/menu。
   - 实现打开 AgentDash、查看状态项、托盘左键恢复窗口。
   - 验证：Windows dev shell 手工点击托盘恢复。

4. `feat(desktop): 关闭窗口隐藏到托盘并区分显式退出`
   - 拦截 close request，默认 hide。
   - explicit quit flag 允许真实退出。
   - 前端关闭按钮继续调用 close，由 Rust lifecycle 接管。
   - 验证：关闭后进程仍在，托盘可恢复；退出后进程结束。

5. `feat(desktop): 增加托盘 runtime 启停与状态刷新`
   - 托盘启动/停止 runtime 调用现有 manager 路径或复用 profile。
   - 菜单根据 runtime snapshot enable/disable。
   - 明确无 profile 时的状态文案/错误。
   - 验证：托盘启动/停止 runtime、设置页状态同步。

6. `feat(desktop): 实现 Windows 开机启动与启动到托盘`
   - `launch_at_login` 控制 autostart enable/disable。
   - `start_minimized_to_tray` 在 setup 阶段尽早 hide 主窗口。
   - 卸载清理 AgentDash startup entry 的路径在设计/validation 中固定。
   - 验证：设置开启后重登/重启启动到托盘；关闭后 startup entry 消失。

7. `fix(desktop): 收敛 runtime 自动连接为单一触发路径`
   - 选择并实现唯一 owner。
   - 移除或禁止 `LocalRuntimeView` mount 自动 start 与 Rust setup 自动 start 的重复路径。
   - `ensureDesktopLocalRuntimeStarted()` 对 `starting/running` 幂等。
   - 验证：启动日志中只出现一次 ensure/claim/start。

8. `fix(desktop): 加固 Desktop API loopback release contract`
   - release/builtin 模式固定 `127.0.0.1`。
   - external/sidecar origin 做 loopback 校验或明确仅 dev。
   - 验证：非 loopback origin 在 release bundle 路径不可生效；`/api/health` 只在 loopback 可达。

9. `feat(desktop): 完善 NSIS bundle metadata 与产物报告`
   - 补 bundle target、installer metadata、图标、shortcut 行为、版本/identifier 稳定性。
   - 构建脚本输出 setup exe/app exe 产物路径。
   - 验证：`pnpm run desktop:bundle`。

10. `test(desktop): 补桌面生命周期与构建验证`
    - 能单测的 settings normalize、origin loopback 校验、runtime auto-connect 幂等逻辑加测试。
    - 手工 Windows 验收清单留给 `distribution-release-validation`。
    - 验证：`pnpm run desktop:check`。

### Handoff Contract: runtime-diagnostics-settings

- 本任务提供 Rust/Tauri shell 能力：托盘、窗口 hide/show/quit、DesktopAppSettings load/save、autostart enable/disable/is_enabled、runtime start/stop/snapshot commands。
- `runtime-diagnostics-settings` 消费这些 API，负责设置页/诊断 UI 的具体呈现：三个桌面偏好开关、Desktop API 状态、runtime 状态、日志、active execution/lease summary。
- `runtime-diagnostics-settings` 不重新实现 tray、close interception、NSIS installer、自启动写入；它只调用本任务暴露的桌面 API。
- 若显式退出需要根据“运行中任务”拒绝或确认，`runtime-diagnostics-settings` 需要把 active execution summary 补进 UI/状态 contract；本任务在没有该 summary 前只能基于 runtime state 做保守处理。
- 两个任务共享 `LocalRuntimeClient` port，但桌面 App 偏好不应塞进通用 `LocalRuntimeClient`，应通过 desktop-only bridge 暴露，避免普通 Web 拿到桌面能力。

### Handoff Contract: distribution-release-validation

- 本任务必须给出稳定构建入口和产物说明：`pnpm run desktop:check`、`pnpm run desktop:bundle`、NSIS setup exe 输出路径、安装后 app exe 路径/进程名。
- `distribution-release-validation` 负责 release 验证流程，不实现桌面功能：安装、启动、关闭到托盘、托盘恢复、显式退出、开机启动、启动到托盘、runtime 自动连接、卸载清理 startup entry、Desktop API loopback 检查。
- 本任务应输出可被验证的版本/identifier/productName/bundle metadata；`distribution-release-validation` 检查这些产物事实是否符合 release contract。
- 代码签名、hash/校验和、发布包命名、干净 VM 验收矩阵属于 `distribution-release-validation`；本任务只保证 installer 能被构建并具备正确生命周期行为。

### Code Patterns

- Tauri main window 当前是无边框自绘标题栏，窗口 label 为 `main`：`crates/agentdash-local-tauri/tauri.conf.json:12` 到 `crates/agentdash-local-tauri/tauri.conf.json:21`。
- Tauri security CSP 当前为 `null`：`crates/agentdash-local-tauri/tauri.conf.json:23` 到 `crates/agentdash-local-tauri/tauri.conf.json:25`。
- Bundle 当前 active 且 targets 为 `all`：`crates/agentdash-local-tauri/tauri.conf.json:27` 到 `crates/agentdash-local-tauri/tauri.conf.json:33`。
- 当前 capabilities 只允许自绘标题栏窗口控制：`crates/agentdash-local-tauri/capabilities/default.json:6` 到 `crates/agentdash-local-tauri/capabilities/default.json:14`。
- Desktop API built-in 默认使用 localhost options：`crates/agentdash-local-tauri/src/main.rs:698` 到 `crates/agentdash-local-tauri/src/main.rs:706`。
- Desktop API origin helper 固定 `127.0.0.1`：`crates/agentdash-local-tauri/src/main.rs:956` 到 `crates/agentdash-local-tauri/src/main.rs:958`。
- Sidecar mode 会按 origin host 设置 `HOST`：`crates/agentdash-local-tauri/src/main.rs:778` 到 `crates/agentdash-local-tauri/src/main.rs:788`。
- `DesktopApiSnapshot` state 枚举符合 spec：`crates/agentdash-local-tauri/src/main.rs:46` 到 `crates/agentdash-local-tauri/src/main.rs:62`。
- `LocalRuntimeProfile` 当前包含 runtime `auto_start`：`crates/agentdash-local-tauri/src/main.rs:92` 到 `crates/agentdash-local-tauri/src/main.rs:116`。
- Rust setup 当前自动调用 profile auto-start：`crates/agentdash-local-tauri/src/main.rs:645` 到 `crates/agentdash-local-tauri/src/main.rs:648`。
- Tauri run loop 当前只在 exit 时 stop sidecar：`crates/agentdash-local-tauri/src/main.rs:670` 到 `crates/agentdash-local-tauri/src/main.rs:673`。
- `DashboardHost` 等 snapshot running 和 `/api/health` ready 后渲染 Web Dashboard：`packages/app-tauri/src/App.tsx:52` 到 `packages/app-tauri/src/App.tsx:96`。
- Tauri runtime bridge 当前只含 LocalRuntimeClient，不含桌面 App settings：`packages/app-tauri/src/runtimeApi.ts:19` 到 `packages/app-tauri/src/runtimeApi.ts:34`。
- Web AuthGate 当前在 currentUser ready 后尝试 runtime auto-start：`packages/app-web/src/App.tsx:215` 到 `packages/app-web/src/App.tsx:222`。
- Settings local runtime panel 只在 Tauri bridge 存在时显示：`packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:52` 到 `packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:65`。
- Desktop build script 默认 builtin API + localhost：`scripts/desktop-build.js:11` 到 `scripts/desktop-build.js:16`。
- `desktop:bundle` 当前使用 NSIS 且不签名：`package.json:24` 到 `package.json:25`。

### External References

- Tauri v2 System Tray docs: https://v2.tauri.app/learn/system-tray/ - 说明 `TrayIconBuilder`/tray click/open-window 模式，页面最后更新时间 Apr 20, 2026。
- Tauri v2 Autostart plugin docs: https://v2.tauri.app/plugin/autostart/ - 说明 autostart plugin、`enable/disable/is_enabled` 和 `autostart:allow-*` permissions，页面最后更新时间 Feb 22, 2025。
- Tauri v2 Config reference: https://v2.tauri.app/reference/config/ - Windows bundle/NSIS/WiX 配置参考，包含 installer UI、version、upgrade code 等配置说明。
- 当前项目版本：`@tauri-apps/cli` 为 `^2.11.1`（`package.json:53` 到 `package.json:55`），Rust `tauri` dependency 为 major 2（`crates/agentdash-local-tauri/Cargo.toml:25`）。

### Related Specs

- `.trellis/spec/cross-layer/desktop-local-runtime.md` - 规定 Desktop API 默认 `127.0.0.1:17301`、DashboardHost 等 health ready、`app-tauri` 复用 `app-web`、Local Runtime UI 依赖 `@agentdash/core` port、Tauri CLI 使用 `pnpm exec tauri`。
- `.trellis/spec/frontend/design-language.md` - 规定新增桌面设置 UI 使用语义 token、有限 radius、`@agentdash/ui` primitives、避免业务字面色和 ad-hoc UI。
- `.trellis/spec/frontend/quality-guidelines.md` - 已在 `check.jsonl` 中登记，适合检查桌面设置交互质量。
- `.trellis/spec/guides/cross-layer-thinking-guide.md` - 已在 `check.jsonl` 中登记，适合检查 Tauri shell、Desktop API、runtime 状态和设置边界。
- `.trellis/spec/backend/logging-guidelines.md` - 已在 `implement.jsonl` 中登记，适合 runtime 日志展示与脱敏检查。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`，本研究根据用户显式提供的任务目录写入 `.trellis/tasks/06-26-windows-desktop-installer-background/research/`。
- 未发现当前代码中已有 tray/autostart 实现；搜索 `tray/SystemTray/TrayIcon/autostart` 仅命中无关或 docs/context。
- 未发现 app-tauri 中已有 desktop settings/autostart/window lifecycle bridge；现有 bridge 只覆盖 Local Runtime、目录浏览、外部链接。
- 未运行构建或测试；本次是 planning reviewer，只做只读探索和 research 记录。
- 未检查 Tauri 实际 bundle 输出目录，因为本任务要求规划审阅，不执行 release build。
