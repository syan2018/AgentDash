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
  - `pnpm run desktop:bundle -- --desktop-defaults <defaults.json>`。
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

- `pnpm run desktop:bundle -- --desktop-defaults <defaults.json>`
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
- Windows Desktop 已实现托盘/后台运行、显式退出、自启动设置、启动到托盘设置、启动后自动连接 runtime 设置、desktop bundle 产物边界输出；默认发行形态连接配置的远端 server，builtin loopback Desktop API `127.0.0.1:17301` 仅作为显式 opt-in 形态保留。
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

- [ ] 准备 desktop defaults JSON，例如 `{ "default_cloud_origin": "https://agentdash.example.com" }`。
- [ ] 在 Windows x64 clean VM 或干净用户环境运行 `pnpm run desktop:bundle -- --desktop-defaults <defaults.json>` 或使用 CI 产出的 NSIS setup exe。
- [ ] 也可用快捷参数 `pnpm run desktop:bundle -- --default-cloud-origin https://agentdash.example.com` 生成同等 defaults。
- [ ] 验证默认构建模式为 `external`，Dashboard API origin 指向 `default_cloud_origin` / `--api-origin` 配置的远端 server。
- [ ] 记录 setup exe 路径、文件名、版本 metadata。
- [ ] 验证安装包携带的 `agentdash-desktop-defaults.json` 包含预期 `default_cloud_origin`。
- [ ] 验证桌面前端运行时实际读取 `agentdash-desktop-defaults.json`，而不是依赖构建期 env 默认值。
- [ ] 从 setup exe 开始安装，验证安装成功。
- [ ] 验证 Start Menu entry 存在。
- [ ] 验证 Desktop shortcut 如安装器配置启用则存在。
- [ ] 启动安装后的 AgentDash app，记录进程名与安装后 app exe 路径。
- [ ] 验证远端 Cloud API health ready，桌面 Dashboard 不启动默认 builtin 本机 API。
- [ ] 如显式测试 builtin/sidecar 形态，验证本机 API 只绑定 `127.0.0.1:17301`，不绑定 LAN 地址或 `0.0.0.0`。
- [ ] 验证 Dashboard 渲染完成。
- [ ] 登录/连接云端后保存本机 runtime profile。
- [ ] 打开本机运行时设置，验证 Server URL 默认预填 desktop defaults 的 `default_cloud_origin`。
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
- [ ] 运行 `agentdash-local setup --server-url <server> --registration-token <token> --runner-name <name> --workspace-root <path> --install-service --start`，记录脱敏 summary。
- [ ] 运行 `agentdash-local doctor --config <config> --json`，验证 config、credentials、service、status、log path 诊断可读且 token 已脱敏。
- [ ] 运行 `agentdash-local status --config <config> --json`，验证 runner 状态可读且 token 已脱敏。
- [ ] 记录 setup 生成/使用的 systemd unit 文件路径。
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
- [ ] 运行 `agentdash-local.exe setup --server-url <server> --registration-token <token> --runner-name <name> --workspace-root <path> --install-service --start`，记录脱敏 summary。
- [ ] 运行 `agentdash-local.exe doctor --config <config> --json`，验证 config、credentials、service、status、log path 诊断可读且 token 已脱敏。
- [ ] 运行 `agentdash-local.exe status --config <config> --json`，验证 runner 状态可读且 token 已脱敏。
- [ ] 验证 SCM service `AgentDashLocalRunner` 创建成功。
- [ ] 验证 SCM binPath 使用 `agentdash-local.exe service run --config <config>`。
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


## Step-By-Step Packaging And Manual Validation Runbook

**0. 前置约定**

所有命令默认在仓库根目录执行：

```powershell
cd C:\Users\yihao.liao\.codex\worktrees\8509\AgentDashboard
```

先确认当前分支和工作区：

```powershell
git status --short --branch
git log --oneline -5
```

预期：

```text
## codex/desktop-local-runtime...origin/codex/desktop-local-runtime
```

并且 `git status --short` 没有脏文件。

记录这些信息到验收 evidence：

```powershell
git rev-parse HEAD
git branch --show-current
```

---

**1. 基础静态检查**

这些是打包前先跑的本地 gate：

```powershell
pnpm install --frozen-lockfile
pnpm run contracts:check
pnpm run migration:guard
cargo fmt --all -- --check
pnpm run desktop:check
```

如果只做 runner release，也建议至少跑：

```powershell
cargo test -p agentdash-local
```

如果要覆盖当前 diagnostics 变更：

```powershell
pnpm --filter app-web run typecheck
pnpm --filter @agentdash/views typecheck
pnpm --filter app-tauri typecheck
pnpm --filter app-web test -- workspaceRouting runtimeDiagnostics
cargo test -p agentdash-local
cargo test -p agentdash-local-tauri
```

注意：当前全量 `pnpm run frontend:lint` 已知会失败在一个无关旧文件：

```text
packages/app-web/src/features/canvas-panel/CanvasRuntimeBindingsEditor.tsx
react-hooks/set-state-in-effect
```

所以这次 release validation 不要把这个误判成本机运行形态任务失败；除非你要顺手另开任务修它。

---

**2. Windows Desktop 完整安装包打包**

这是用户最终下载/安装的完整桌面安装包。

在 Windows 环境执行：

```powershell
pnpm run desktop:bundle
```

这个脚本实际对应：

```json
"desktop:bundle": "node ./scripts/desktop-build.js --bundles nsis --no-sign --ci"
```

预期产物：

```text
target\release\bundle\nsis\*.exe
```

也就是 NSIS setup exe。这个是“交付给普通 Windows 用户安装”的产物。

同时构建后可能存在 app exe 候选：

```text
target\release\AgentDash.exe
target\release\agentdash-local-tauri.exe
```

这里要记住边界：

- `target\release\bundle\nsis\*.exe` 是安装包。
- `target\release\AgentDash.exe` / `agentdash-local-tauri.exe` 是安装后的 app 进程候选或 release app exe。
- release 验收必须从 NSIS setup exe 开始，不能只双击 app exe 代替安装流程。

打包后记录：

```powershell
Get-ChildItem .\target\release\bundle\nsis\*.exe | Select-Object FullName, Length, LastWriteTime
Get-ChildItem .\target\release\*.exe | Select-Object FullName, Length, LastWriteTime
```

可选记录 hash：

```powershell
Get-FileHash .\target\release\bundle\nsis\*.exe -Algorithm SHA256
```

---

**3. Windows Desktop app exe 单独构建**

这个不是用户最终安装包，主要用于诊断“release app exe 是否能构建”。

```powershell
pnpm run desktop:build
```

这个脚本实际对应：

```json
"desktop:build": "node ./scripts/desktop-build.js --no-bundle --ci"
```

预期产物：

```text
target\release\AgentDash.exe
target\release\agentdash-local-tauri.exe
```

记录：

```powershell
Get-ChildItem .\target\release\AgentDash.exe, .\target\release\agentdash-local-tauri.exe -ErrorAction SilentlyContinue |
  Select-Object FullName, Length, LastWriteTime
```

验收说明：

- 这个命令可以证明 desktop app release binary 能出来。
- 它不能替代 NSIS installer 安装/卸载验收。
- 自启动、卸载清理、Start Menu entry、安装目录，都必须通过 `desktop:bundle` 的 setup exe 测。

---

**4. Windows Desktop 安装包手工验收**

在干净 Windows x64 VM 或至少干净用户环境中执行。

1. 运行安装包：

```powershell
.\target\release\bundle\nsis\<AgentDash-setup>.exe
```

2. 验证安装成功：

```powershell
Get-Process AgentDash -ErrorAction SilentlyContinue
Get-Process agentdash-local-tauri -ErrorAction SilentlyContinue
```

3. 验证 Desktop API 只在 loopback：

```powershell
netstat -ano | Select-String "17301"
```

预期看到 `127.0.0.1:17301`，不应该看到 `0.0.0.0:17301` 或 LAN IP。

如果 app 已启动，可测 health：

```powershell
Invoke-WebRequest http://127.0.0.1:17301/api/health
```

4. 验证启动项：

开启 `launch_at_login=true` 后检查：

```powershell
Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" | Select-Object AgentDash
```

预期：

- 有 `AgentDash` 项。
- 值指向安装后的 app exe。
- 不指向 setup/installer exe。

5. 验证关闭到托盘：

手工步骤：

- 打开 AgentDash。
- 点击窗口关闭按钮。
- 预期窗口隐藏，但进程仍存在。
- 通过托盘菜单 `Open AgentDash` 恢复窗口。
- 如果 runtime 正在运行，关闭窗口不应中断 runtime。

6. 验证显式退出：

手工步骤：

- 从托盘菜单选择 Quit / Exit。
- 预期 Tauri 进程退出。
- 桌面托管 runtime 被停止。

可辅助检查：

```powershell
Get-Process AgentDash -ErrorAction SilentlyContinue
Get-Process agentdash-local-tauri -ErrorAction SilentlyContinue
```

7. 验证启动偏好：

在设置页分别测试：

- `launch_at_login`
- `start_minimized_to_tray`
- `auto_connect_local_runtime`

建议记录成表：

```text
设置项 | 操作 | 重启/重新登录后结果 | 证据
launch_at_login=true | 重新登录 | app 自动启动 | HKCU Run + 进程截图
start_minimized_to_tray=true | 重启 app | 主窗口不弹出，托盘存在 | 截图
auto_connect_local_runtime=true | 重启 app | runtime 自动连接 | 诊断页 + 云端 online
auto_connect_local_runtime=false | 重启 app | runtime 不自动连接 | 诊断页
```

8. 验证卸载：

通过 Windows Apps / NSIS uninstaller 卸载后检查：

```powershell
Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" | Select-Object AgentDash
Get-Process AgentDash -ErrorAction SilentlyContinue
Get-Process agentdash-local-tauri -ErrorAction SilentlyContinue
```

预期：

- AgentDash 管理的 HKCU Run entry 被清理。
- app 进程退出。
- 安装目录、Start Menu entry、Desktop shortcut 按安装器配置清理。
- workspace、任务产物、machine identity、profile、logs/cache 默认保留，不被静默删除。

---

**5. Windows Runner release binary 打包**

这是服务器托管场景下的独立 runner，无 UI。

在 Windows 执行：

```powershell
cargo build -p agentdash-local --release
```

预期产物：

```text
target\release\agentdash-local.exe
```

记录版本：

```powershell
.\target\release\agentdash-local.exe --version
```

记录文件和 hash：

```powershell
Get-ChildItem .\target\release\agentdash-local.exe | Select-Object FullName, Length, LastWriteTime
Get-FileHash .\target\release\agentdash-local.exe -Algorithm SHA256
```

可选打 zip：

```powershell
New-Item -ItemType Directory -Force .\dist | Out-Null
Compress-Archive -Path .\target\release\agentdash-local.exe -DestinationPath .\dist\agentdash-local-windows-x64.zip -Force
Get-FileHash .\dist\agentdash-local-windows-x64.zip -Algorithm SHA256
```

环境专用 runner artifact 可以在构建时写入非密钥默认 server origin：

```powershell
$env:AGENTDASH_RUNNER_DEFAULT_SERVER_URL="https://<your-agentdash-server>"
cargo build -p agentdash-local --release
Remove-Item Env:\AGENTDASH_RUNNER_DEFAULT_SERVER_URL
```

该默认值只用于减少 `setup` 输入项；registration token 和 claim 后返回的 relay credentials 仍来自运行期。

---

**6. Windows Runner 一键 setup**

创建一个目录：

```powershell
New-Item -ItemType Directory -Force C:\AgentDash\runner | Out-Null
New-Item -ItemType Directory -Force C:\AgentDash\runner\state | Out-Null
New-Item -ItemType Directory -Force C:\AgentDash\runner\logs | Out-Null
New-Item -ItemType Directory -Force C:\AgentDash\workspaces | Out-Null
```

管理员 PowerShell 运行：

```powershell
.\target\release\agentdash-local.exe setup `
  --config C:\AgentDash\runner\agentdash-runner.toml `
  --server-url https://<your-agentdash-server> `
  --registration-token adrt_<token_id>_<secret> `
  --runner-name windows-runner-01 `
  --workspace-root C:\AgentDash\workspaces `
  --install-service `
  --start
```

如果 binary 已内嵌 `AGENTDASH_RUNNER_DEFAULT_SERVER_URL`，命令可以省略 `--server-url`：

```powershell
.\target\release\agentdash-local.exe setup `
  --config C:\AgentDash\runner\agentdash-runner.toml `
  --registration-token adrt_<token_id>_<secret> `
  --runner-name windows-runner-01 `
  --workspace-root C:\AgentDash\workspaces `
  --install-service `
  --start
```

诊断：

```powershell
.\target\release\agentdash-local.exe doctor --config C:\AgentDash\runner\agentdash-runner.toml --json
.\target\release\agentdash-local.exe status --config C:\AgentDash\runner\agentdash-runner.toml --json
```

---

**7. Windows Runner 配置文件诊断示例**

setup 会写入配置文件；需要人工排障时可打开：

```powershell
notepad C:\AgentDash\runner\agentdash-runner.toml
```

示例内容按当前 runner 设计应包含这些信息。字段名如果后续实现里已有更精确模板，以 `agentdash-local status --json` 和实际 config parser 为准：

```toml
[runner]
name = "windows-runner-01"
server_url = "https://<your-agentdash-server>"
state_dir = "C:\\AgentDash\\runner\\state"
log_path = "C:\\AgentDash\\runner\\logs\\agentdash-local.log"
workspace_roots = ["C:\\AgentDash\\workspaces"]
executor_enabled = true

[registration]
token = "adrt_<token_id>_<secret>"
```

如果已经完成 claim，也可能会写回类似这些 server-issued credentials：

```toml
[credentials]
backend_id = "<server-issued-backend-id>"
relay_ws_url = "wss://<your-agentdash-server>/ws/backend"
auth_token = "<server-issued-relay-auth-token>"
```

注意：

- `registration_token` 只用于 claim。
- WebSocket relay 连接使用 claim 返回的 `auth_token`。
- 不要把 registration token 当 relay token 用。

---

**8. Windows Runner 前台 smoke test**

先不要装服务，前台跑一遍：

```powershell
.\target\release\agentdash-local.exe status --config C:\AgentDash\runner\agentdash-runner.toml --json
.\target\release\agentdash-local.exe run --config C:\AgentDash\runner\agentdash-runner.toml
```

预期：

- 首次 claim 成功。
- 配置写回 `backend_id / relay_ws_url / auth_token`。
- 云端 backend/runtime summary 看到 runner online。
- 日志和 status 输出不泄露 token。

另开 PowerShell 检查：

```powershell
.\target\release\agentdash-local.exe status --config C:\AgentDash\runner\agentdash-runner.toml --json
Get-Content C:\AgentDash\runner\logs\agentdash-local.log -Tail 80
```

---

**9. Windows Runner 安装为 Windows Service**

必须使用管理员 PowerShell。

安装：

```powershell
.\target\release\agentdash-local.exe service install --config C:\AgentDash\runner\agentdash-runner.toml
```

启动：

```powershell
.\target\release\agentdash-local.exe service start --config C:\AgentDash\runner\agentdash-runner.toml
```

状态：

```powershell
.\target\release\agentdash-local.exe service status --config C:\AgentDash\runner\agentdash-runner.toml
Get-Service AgentDashLocalRunner
sc.exe qc AgentDashLocalRunner
```

预期：

- `Get-Service AgentDashLocalRunner` 显示 `Running`。
- `sc.exe qc` 里 binPath 使用：
  ```text
  agentdash-local.exe service run --config <config>
  ```
- 云端 backend/runtime summary online。
- `registration_source=runner_registration_token`。
- 日志脱敏。

停止：

```powershell
.\target\release\agentdash-local.exe service stop --config C:\AgentDash\runner\agentdash-runner.toml
Get-Service AgentDashLocalRunner
```

卸载：

```powershell
.\target\release\agentdash-local.exe service uninstall --config C:\AgentDash\runner\agentdash-runner.toml
Get-Service AgentDashLocalRunner
```

最后一条预期找不到 service。

确认进程退出：

```powershell
Get-Process agentdash-local -ErrorAction SilentlyContinue
```

---

**10. Linux Runner release binary 打包**

建议在 Linux x64 host 或 CI 上构建，不建议临时从 Windows 跨编译，除非已经配好 Rust target、linker 和系统依赖。

在 Linux 仓库根目录：

```bash
pnpm install --frozen-lockfile
cargo build -p agentdash-local --release
```

预期产物：

```bash
target/release/agentdash-local
```

记录版本：

```bash
./target/release/agentdash-local --version
```

记录文件和 hash：

```bash
ls -lh target/release/agentdash-local
sha256sum target/release/agentdash-local
```

可选打 tarball：

```bash
mkdir -p dist
tar -czf dist/agentdash-local-linux-x64.tar.gz -C target/release agentdash-local
sha256sum dist/agentdash-local-linux-x64.tar.gz
```

环境专用 runner artifact 可以在构建时写入非密钥默认 server origin：

```bash
AGENTDASH_RUNNER_DEFAULT_SERVER_URL="https://<your-agentdash-server>" cargo build -p agentdash-local --release
```

该默认值只用于 setup 默认提示或省略 `--server-url`；registration token 和 relay credentials 仍来自运行期。

---

**11. Linux Runner 一键 setup**

准备目录：

```bash
sudo mkdir -p /etc/agentdash
sudo mkdir -p /var/lib/agentdash-local-runner
sudo mkdir -p /var/log/agentdash
sudo mkdir -p /srv/agentdash-workspaces
```

复制 binary：

```bash
sudo install -m 0755 target/release/agentdash-local /usr/local/bin/agentdash-local
agentdash-local --version
```

运行 setup：

```bash
sudo agentdash-local setup \
  --config /etc/agentdash/agentdash-runner.toml \
  --server-url https://<your-agentdash-server> \
  --registration-token adrt_<token_id>_<secret> \
  --runner-name linux-runner-01 \
  --workspace-root /srv/agentdash-workspaces \
  --install-service \
  --start
```

如果 binary 已内嵌 `AGENTDASH_RUNNER_DEFAULT_SERVER_URL`，命令可以省略 `--server-url`：

```bash
sudo agentdash-local setup \
  --config /etc/agentdash/agentdash-runner.toml \
  --registration-token adrt_<token_id>_<secret> \
  --runner-name linux-runner-01 \
  --workspace-root /srv/agentdash-workspaces \
  --install-service \
  --start
```

诊断：

```bash
agentdash-local doctor --config /etc/agentdash/agentdash-runner.toml --json
agentdash-local status --config /etc/agentdash/agentdash-runner.toml --json
```

---

**12. Linux Runner 配置文件诊断示例**

setup 会写入配置文件；需要人工排障时可打开：

```bash
sudo nano /etc/agentdash/agentdash-runner.toml
```

示例：

```toml
[runner]
name = "linux-runner-01"
server_url = "https://<your-agentdash-server>"
state_dir = "/var/lib/agentdash-local-runner"
log_path = "/var/log/agentdash/agentdash-local.log"
workspace_roots = ["/srv/agentdash-workspaces"]
executor_enabled = true

[registration]
token = "adrt_<token_id>_<secret>"
```

检查状态：

```bash
agentdash-local status --config /etc/agentdash/agentdash-runner.toml --json
```

---

**13. Linux Runner 前台 smoke test**

```bash
agentdash-local run --config /etc/agentdash/agentdash-runner.toml
```

另一个 shell：

```bash
agentdash-local status --config /etc/agentdash/agentdash-runner.toml --json
tail -n 80 /var/log/agentdash/agentdash-local.log
```

预期：

- claim 成功。
- config 写回 server-issued credentials。
- 云端显示 online。
- 日志和 status 无明文 token。

---

**14. Linux Runner 安装为 systemd service**

安装：

```bash
sudo agentdash-local service install --config /etc/agentdash/agentdash-runner.toml
```

启动：

```bash
sudo agentdash-local service start --config /etc/agentdash/agentdash-runner.toml
```

状态：

```bash
agentdash-local service status --config /etc/agentdash/agentdash-runner.toml
systemctl status agentdash-local-runner --no-pager
journalctl -u agentdash-local-runner -n 100 --no-pager
```

预期：

- systemd service active/running。
- 云端 backend/runtime summary online。
- `registration_source=runner_registration_token`。
- `runner-status.json` 显示 relay state。
- 日志无 token 泄露。

停止：

```bash
sudo agentdash-local service stop --config /etc/agentdash/agentdash-runner.toml
systemctl status agentdash-local-runner --no-pager
```

卸载：

```bash
sudo agentdash-local service uninstall --config /etc/agentdash/agentdash-runner.toml
systemctl status agentdash-local-runner --no-pager
```

预期：

- unit 不存在或 not found。
- runner 进程退出。
- config、credentials、state、logs、workspace data 默认保留。

辅助检查：

```bash
pgrep -a agentdash-local || true
ls -la /etc/agentdash
ls -la /var/lib/agentdash-local-runner
ls -la /var/log/agentdash
```

---

**15. 断网重连验收**

Linux 可用一种简单方式临时阻断 server origin，例如如果 server 是固定 host/IP，可以用防火墙规则。更温和的方式是直接断开测试 VM 网络。

验收步骤：

1. runner 正常 online。
2. 断开网络或阻断 server。
3. 观察：

```bash
agentdash-local status --config /etc/agentdash/agentdash-runner.toml --json
journalctl -u agentdash-local-runner -n 100 --no-pager
```

Windows：

```powershell
.\target\release\agentdash-local.exe service status --config C:\AgentDash\runner\agentdash-runner.toml
Get-Content C:\AgentDash\runner\logs\agentdash-local.log -Tail 100
```

预期：

- runner 进入 reconnecting / retrying / offline 类状态。
- 云端 online 状态变为离线或不可派发。
- 日志有可诊断错误，但无 token。

4. 恢复网络。
5. 预期自动 online，不需要手工重启 service。

---

**16. Diagnostics UI 验收**

打开 Windows Desktop App 的设置页，本机运行时/诊断入口。

逐项验证：

```text
Cloud API
Desktop API
Local Runtime
Runner
Relay
Registration
Logs
Desktop Settings
```

要点：

- Cloud API 异常时，Cloud 层显示异常。
- Desktop API starting/running/error/stopped 显示准确。
- Local Runtime stopped/running/error 显示准确。
- Relay 状态来自 `LocalRuntimeStatus.relay_connection` 结构化 snapshot。
- 不允许从日志文本或 `backend.online` 推断 relay。
- Registration source 显示：
  - 桌面登录授权：`desktop_access_token`
  - Runner 注册令牌：`runner_registration_token`
- 独立 runner 显示为 systemd / Windows Service 管理，不提供桌面 UI 假重启按钮。
- 桌面托管 runtime 可以在 UI 里 start / stop / restart。
- 有 running/canceling session 时 restart runtime，要显示阻止重启文案。

日志复制验证：

1. 打开诊断日志。
2. 点击复制。
3. 粘贴到临时文本文件。
4. 搜索：

```powershell
Select-String -Path .\copied-logs.txt -Pattern "token|access_token|refresh_token|auth_token|relay_token|registration_token|Bearer"
```

预期：

- 可以看到字段名。
- 不应该看到真实 secret 值。
- 值应为 `***` 或等价脱敏形式。

---

**17. 最终 release gate 判定**

这些必须全部通过才能归档 `distribution-release-validation`：

- [ ] `pnpm run desktop:bundle` 产出 NSIS setup exe。
- [ ] Windows installer 可安装、启动、后台运行、托盘恢复、显式退出。
- [ ] Desktop API 只绑定 `127.0.0.1:17301`。
- [ ] Windows login autostart 行为通过真实重启/重新登录验证。
- [ ] Windows uninstall 清理安装项和 AgentDash 管理的 HKCU Run entry。
- [ ] Windows runner release binary 可作为 Windows Service online。
- [ ] Linux runner release binary 可作为 systemd service online。
- [ ] Linux/Windows runner 可通过 `agentdash-local setup ... --install-service --start` 完成首次部署。
- [ ] Linux/Windows runner 的 `agentdash-local doctor --json` 能输出脱敏诊断 summary。
- [ ] Linux/Windows runner 断网后自动重连。
- [ ] 三类产物版本证据一致。
- [ ] diagnostics UI 能定位 Cloud / Desktop API / Runtime / Runner / Relay / Registration 问题。
- [ ] 日志显示和复制不泄露 token。
- [ ] 所有阻断项 evidence 都有命令输出、截图或日志路径。

完成后归档：

```powershell
python ./.trellis/scripts/task.py archive 06-26-distribution-release-validation
```

如果父任务此时显示 `5/5 done`，再归档父任务：

```powershell
python ./.trellis/scripts/task.py archive 06-26-local-runtime-distribution
```
