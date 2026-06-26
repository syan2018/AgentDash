# 发布产物与验收流程 - Implement

## Step 1 - Release Planning Docs

- Expand `design.md` with artifact matrix、version contract、installer/app exe boundary、runner release contract、service validation、cleanup boundary、manual acceptance template、release gate。
- Expand `implement.md` with concrete handoff gates and validation steps。
- Link subtask research files in context manifests。

Validation:

- Trellis `task.py validate` for parent and child tasks。

## Step 2 - Version Consistency Check

- Add or document release version check:
  - root `package.json`。
  - Cargo workspace。
  - Tauri config。
  - runner binary。
  - generated protocol/contracts。
- Define evidence file or release notes section to record results。

Validation:

- version check command once implemented。
- `pnpm run contracts:check`。

## Step 3 - Windows Desktop Release Artifact

- Consume desktop handoff:
  - `pnpm run desktop:bundle`。
  - output glob/path。
  - setup exe name。
  - installed app exe/process name。
  - metadata。
- Write Windows Desktop checklist:
  - install。
  - launch。
  - Desktop API health。
  - Dashboard render。
  - close-to-tray。
  - tray restore。
  - explicit quit。
  - launch at login。
  - start to tray。
  - auto-connect runtime。
  - uninstall cleanup。

Validation:

- `pnpm run desktop:bundle`
- clean Windows manual acceptance。

## Step 4 - Linux Runner Release Artifact

- Consume runner handoff:
  - release build command。
  - binary path。
  - config example。
  - systemd service command。
  - log/status paths。
  - version command。
- Write Linux checklist。

Validation:

- release binary exists。
- `agentdash-local --version`。
- systemd install/start/status/stop/uninstall。
- cloud online/offline/reconnect evidence。

## Step 5 - Windows Runner Release Artifact

- Consume runner handoff:
  - release build command。
  - binary path。
  - Windows Service install command。
  - service name。
  - log/status paths。
  - version command。
- Write Windows Runner checklist。

Validation:

- admin PowerShell service lifecycle。
- cloud online/offline/reconnect evidence。

## Step 6 - Cleanup Validation

- Desktop:
  - installation directory removed。
  - shortcuts removed。
  - uninstall registry entry removed。
  - AgentDash startup entry removed。
  - process exited。
- Linux runner:
  - systemd unit removed/disabled。
  - process exited。
  - config/log/data preserved unless explicit purge。
- Windows runner:
  - service registration removed。
  - process exited。
  - config/log/data preserved unless explicit purge。

Validation:

- platform commands/screenshots recorded in acceptance evidence。

## Step 7 - Manual Acceptance Workbook

- Create final checklist table for:
  - Windows Desktop。
  - Linux Runner。
  - Windows Runner。
- Each row includes:
  - action。
  - expected result。
  - evidence。
  - diagnostics。
  - gate severity。
- Mark unavailable platforms as blocked by environment, not passed。

## Step 8 - Release Gate Dry Run

- Run build/version/contracts gates。
- Execute available platform acceptance。
- Record failures as blocking/warning/info。
- Do not mark release validation done until all blocking gates pass。

## Blockers Before Start

- Windows Desktop task must output real installer path and lifecycle handoff。
- Local Runner task must output Linux/Windows service commands and version command。
- Diagnostics task must output recovery/log paths。

## Current Handoff Baseline

截至 2026-06-26，已由上游子任务交付或部分交付的内容：

- Runner enrollment token 已归档，registration token 管理 API、runner claim API、claim DTO、ProjectBackendAccess side effect 与 relay auth 边界已实现。
- Local Runner 已实现配置合并、registration token claim、凭据写回、status snapshot、文件日志、relay state、Linux systemd service command 与 Windows SCM service command。
- Windows Desktop 已实现 Desktop API loopback 端口 `127.0.0.1:17301`、托盘/后台运行、显式退出、自启动设置、启动到托盘设置、启动后自动连接 runtime 设置、desktop bundle 产物边界输出。
- Runtime diagnostics 正在接入状态聚合、设置入口、日志与恢复动作；完成后本任务应消费其 UI 路径、日志脱敏证据与恢复命令。

本任务仍然不能归档的原因：

- Windows NSIS installer 安装/卸载、自启动登录后行为、托盘交互需要真实 Windows 桌面环境验收。
- Linux systemd service 生命周期、断网重连、云端 online 投影需要真实 Linux systemd host 和可访问云端环境验收。
- Windows Service 生命周期、断网重连、云端 online 投影需要管理员 PowerShell 与可访问云端环境验收。
- 最终 release gate 必须记录版本一致性证据和每个阻断步骤的 evidence。

## Operator Manual Validation Checklist

下面 checklist 是交给后续接手者逐项执行的最终验收清单。每一项都应记录执行环境、命令/动作、结果、证据路径或截图、失败诊断入口。未执行的项保持 unchecked，不以本地单元测试替代。

### A. Preflight / Version Consistency

- [ ] 记录 release commit SHA。
- [ ] 记录 `git status --short` 为空。
- [ ] 运行 `pnpm run contracts:check` 并记录输出。
- [ ] 运行 `pnpm run migration:guard` 并记录输出。
- [ ] 运行 `cargo fmt --all -- --check` 并记录输出。
- [ ] 记录 root `package.json` version。
- [ ] 记录 Cargo workspace/package version。
- [ ] 记录 `crates/agentdash-local-tauri/tauri.conf.json` version。
- [ ] 构建 release runner 后记录 `agentdash-local --version` / `agentdash-local.exe --version`。
- [ ] 验证云端 backend/runtime summary 中展示的 client/runtime version 与 release 版本一致。
- [ ] 验证 generated contracts 来自同一源码版本。

### B. Windows Desktop Installer

- [ ] 在 Windows x64 clean VM 或干净用户环境运行 `pnpm run desktop:bundle` 或使用 CI 产出的 NSIS setup exe。
- [ ] 记录 setup exe 路径、文件名、版本 metadata。
- [ ] 从 setup exe 开始安装，验证安装成功。
- [ ] 验证 Start Menu entry 存在。
- [ ] 验证 Desktop shortcut 如安装器配置启用则存在。
- [ ] 启动安装后的 AgentDash app，记录进程名与安装后 app exe 路径。
- [ ] 验证 Desktop API health ready，且绑定 `127.0.0.1:17301`。
- [ ] 验证 Desktop API 未绑定 LAN 地址或 `0.0.0.0`。
- [ ] 验证 Dashboard 渲染完成。
- [ ] 登录/连接云端后保存本机 runtime profile。
- [ ] 启动 Local Runtime，验证云端可见 backend online。
- [ ] 关闭主窗口，验证窗口隐藏到托盘且 runtime 不被中断。
- [ ] 从托盘 Open AgentDash 恢复窗口。
- [ ] 从托盘启动/停止 runtime，验证状态展示准确。
- [ ] 设置 `launch_at_login=true`，验证 HKCU Run entry 指向安装后的 app exe，不指向 setup/installer exe。
- [ ] 重启或重新登录 Windows，验证 app 按设置自启动。
- [ ] 设置 `start_minimized_to_tray=true` 后重新启动，验证启动后驻留托盘。
- [ ] 设置 `auto_connect_local_runtime=true` 后重新启动，验证 runtime 自动连接。
- [ ] 设置 `auto_connect_local_runtime=false` 后重新启动，验证 runtime 不自动连接。
- [ ] 显式 Quit AgentDash，验证 runtime 被停止、Tauri 进程退出。
- [ ] 卸载 AgentDash，验证安装目录、Start Menu entry、Desktop shortcut、uninstall registry entry 被清理。
- [ ] 卸载后验证 AgentDash 管理的 HKCU Run entry 被清理。
- [ ] 卸载后验证用户 workspace、任务产物、machine identity/profile/logs 按保留边界未被默认删除。

### C. Linux Local Runner Service

- [ ] 在 Linux x64 systemd host 准备可访问云端 server origin。
- [ ] 使用云端 UI/API 创建 project-scoped runner registration token。
- [ ] 构建或下载 release `agentdash-local` binary，不使用 `target/debug` 或 repo dev script。
- [ ] 运行 `agentdash-local --version` 并记录输出。
- [ ] 创建 runner config，包含 `server_url`、registration token 或已领取 credentials、runner name、workspace roots。
- [ ] 运行 `agentdash-local status --config <config> --json`，验证缺省/未连接状态可读且 token 已脱敏。
- [ ] 运行 `agentdash-local service install --config <config>`，记录 unit 文件路径。
- [ ] 运行 `agentdash-local service start --config <config>` 或 `systemctl start agentdash-local-runner`。
- [ ] 运行 `agentdash-local service status --config <config>`，验证 running/active。
- [ ] 运行 `systemctl status agentdash-local-runner`，验证 active/running。
- [ ] 验证 runner 首次 claim 后将 `backend_id`、`relay_ws_url`、`auth_token` 写回本地配置。
- [ ] 验证本地配置、status 输出和日志中不出现明文 registration token、access token、refresh token、auth token。
- [ ] 验证云端 backend/runtime summary 显示 runner online。
- [ ] 验证 runner `registration_source=runner_registration_token`。
- [ ] 验证 relay state 从 connecting/registered 进入在线状态。
- [ ] 临时断网或阻断 server origin，验证 runner 进入 reconnect/offline，日志给出可诊断错误。
- [ ] 恢复网络，验证 runner 自动重连并在云端恢复 online。
- [ ] 运行 `agentdash-local service stop --config <config>` 或 `systemctl stop agentdash-local-runner`，验证进程退出。
- [ ] 运行 `agentdash-local service uninstall --config <config>`，验证 systemd unit 被移除/disable。
- [ ] 卸载后验证 config、credentials、machine identity、workspace data、logs/cache 默认保留。

### D. Windows Local Runner Service

- [ ] 在 Windows x64 host 使用管理员 PowerShell。
- [ ] 准备可访问云端 server origin。
- [ ] 使用云端 UI/API 创建 project-scoped runner registration token。
- [ ] 构建或下载 release `agentdash-local.exe` binary，不使用 debug/dev script。
- [ ] 运行 `agentdash-local.exe --version` 并记录输出。
- [ ] 创建 runner config，包含 `server_url`、registration token 或已领取 credentials、runner name、workspace roots。
- [ ] 运行 `agentdash-local.exe status --config <config> --json`，验证缺省/未连接状态可读且 token 已脱敏。
- [ ] 运行 `agentdash-local.exe service install --config <config>`，验证 SCM service `AgentDashLocalRunner` 创建成功。
- [ ] 验证 SCM binPath 使用 `agentdash-local.exe service run --config <config>`。
- [ ] 运行 `agentdash-local.exe service start --config <config>` 或 `Start-Service AgentDashLocalRunner`。
- [ ] 运行 `agentdash-local.exe service status --config <config>`，验证 running。
- [ ] 运行 `Get-Service AgentDashLocalRunner`，验证 `Status = Running`。
- [ ] 验证 runner 首次 claim 后将 `backend_id`、`relay_ws_url`、`auth_token` 写回本地配置。
- [ ] 验证本地配置、status 输出和日志中不出现明文 registration token、access token、refresh token、auth token。
- [ ] 验证云端 backend/runtime summary 显示 runner online。
- [ ] 验证 runner `registration_source=runner_registration_token`。
- [ ] 临时断网或阻断 server origin，验证 runner 进入 reconnect/offline。
- [ ] 恢复网络，验证 runner 自动重连并在云端恢复 online。
- [ ] 运行 `agentdash-local.exe service stop --config <config>` 或 `Stop-Service AgentDashLocalRunner`，验证进程退出。
- [ ] 运行 `agentdash-local.exe service uninstall --config <config>`，验证 SCM service 被删除。
- [ ] 卸载后验证 config、credentials、machine identity、workspace data、logs/cache 默认保留。

### E. Runtime Diagnostics / Settings

- [ ] 在桌面 app 中打开本机运行诊断入口。
- [ ] 验证 Cloud API 正常/异常时显示对应层状态和恢复文案。
- [ ] 验证 Desktop API starting/running/error/stopped 显示对应层状态和恢复文案。
- [ ] 验证 Local Runtime stopped/running/error 显示对应层状态和恢复文案。
- [ ] 验证 Runner online/offline 或 read-only service-managed 状态表达清楚。
- [ ] 验证 Relay connecting/registered/reconnecting/disconnected/error 状态来自结构化 snapshot，不从日志文本推断。
- [ ] 验证 registration source 显示桌面登录授权或 Runner 注册令牌。
- [ ] 验证 backend id、machine id、profile/scope/capability slot、last seen/connected 时间展示准确。
- [ ] 点击刷新，验证各层状态重新加载。
- [ ] 点击 restart runtime，验证成功路径。
- [ ] 当前有 active session 时点击 restart runtime，验证提示“当前有会话正在运行，结束后再重启 runtime”或等价文案。
- [ ] 点击 stop runtime，验证状态更新。
- [ ] 查看 logs tail，验证日志内容已脱敏。
- [ ] 复制 logs，验证剪贴板内容不包含 token、access token、refresh token、auth token、registration token。
- [ ] 清空 logs，验证日志区刷新为空或显示清空后状态。
- [ ] 修改 `launch_at_login`，验证 UI 状态与系统自启动状态一致。
- [ ] 修改 `start_minimized_to_tray`，验证重启后行为一致。
- [ ] 修改 `auto_connect_local_runtime`，验证重启后 runtime 自动连接行为一致。

### F. Release Gate Decision

- [ ] 所有 block gate 通过后，将本任务标记为可归档。
- [ ] 任一 block gate 失败时，记录失败项、环境、日志路径、截图和诊断入口。
- [ ] warn/info gate 失败时，记录 release note 或 follow-up task。
- [ ] 最终归档前更新父任务 `local-runtime-distribution` 的完成状态和 handoff。

## Risk Checks

- No dev-runtime or target/debug path appears in final user release flow。
- Setup exe is not confused with app exe。
- User workspace/task data is not deleted by uninstall。
- Version evidence matches across artifacts。
