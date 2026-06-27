# 发布产物与验收流程 - Design

## Scope

本子任务只负责发布产物定义、版本一致性、release gate 与手工验收流程。它不重新设计 Desktop runtime、runner service manager、registration token 或诊断 UI，而是消费其他子任务的 handoff。

发布流程不得依赖 `pnpm dev`、`target/debug`、dev-runtime 或源码 checkout 中的临时启动链路。

## Release Artifact Matrix

| Artifact | Platform | Deliverable | User Entry | Build Command | Version Evidence | Diagnostics | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Windows Desktop Installer | Windows x64 | NSIS setup exe | installer + installed app | `pnpm run desktop:bundle` | installer metadata + app version | Desktop API `/api/health`, desktop runtime logs | required |
| Linux Local Runner | Linux x64 | `agentdash-local` release binary + service install docs | systemd service | from runner handoff | `agentdash-local --version`, cloud runtime version | `journalctl`, runner log, cloud backend health | required |
| Windows Local Runner | Windows x64 | `agentdash-local.exe` release binary + service install docs | Windows Service | from runner handoff | `agentdash-local.exe --version`, cloud runtime version | Windows Event Log/runner log/cloud backend health | required |

## Version Consistency Contract

单次 release 只有一个 `release_version`。必须校验：

- `package.json.version`
- Cargo workspace version
- `crates/agentdash-local-tauri/tauri.conf.json.version`
- runner binary version
- desktop ensure/claim `client_version`
- runner WebSocket register version
- generated protocol/contracts 来自同一源码版本

Release evidence：

- 构建前 version check。
- Desktop installer/app version。
- runner `--version` output。
- cloud runtime health / backend summary 中的 client/runtime version。
- contracts check result。

## Desktop Installer vs App Exe

用户交付对象是 NSIS setup exe。安装后的 app exe 是 runtime process，但不是用户下载交付对象。验收必须从干净 Windows 环境运行 installer 开始，覆盖：

- installation。
- Start Menu/Desktop entry。
- app launch。
- Desktop API health。
- Dashboard render。
- local runtime profile save/start。
- close-to-tray。
- login autostart。
- explicit quit。
- uninstall cleanup。

直接运行 app exe 只能作为诊断入口，不能替代 installer验收。

## Runner Release Contract

Runner release 产物必须满足：

- 不依赖 repo checkout。
- 不依赖 `target/debug`。
- 不依赖 dev-runtime。
- 提供版本查看命令。
- 通过 config/env/service install 参数运行。
- 通过 registration token 或 server-issued credentials 完成上线。

Runner handoff 必须给出：

- binary name and output path。
- config path。
- service name。
- install/start/status/stop/uninstall commands。
- log path。
- status command。
- network interruption expected behavior。

## Linux Service Validation

前置：

- Linux x64。
- systemd。
- root/sudo install 权限。
- 可访问云端 server origin。

步骤：

1. 安装 runner binary 和 config。
2. 安装 systemd service。
3. `systemctl start agentdash-local-runner`。
4. `systemctl status agentdash-local-runner` 为 active/running。
5. 云端 backend/runtime summary 显示 online，version 匹配。
6. 断网后进入 reconnect/offline 预期状态。
7. 恢复网络后自动 online。
8. `systemctl stop` + uninstall。
9. service 不存在且 runner 进程退出。

## Windows Service Validation

前置：

- Windows x64。
- 管理员 PowerShell。
- 可访问云端 server origin。

步骤：

1. 安装 runner binary 和 config。
2. 注册 `AgentDashLocalRunner` 服务。
3. `Start-Service AgentDashLocalRunner`。
4. `Get-Service AgentDashLocalRunner` 为 Running。
5. 云端 backend/runtime summary 显示 online，version 匹配。
6. 断网后进入 reconnect/offline 预期状态。
7. 恢复网络后自动 online。
8. `Stop-Service` + uninstall/delete service。
9. service 不存在且 runner 进程退出。

## Uninstall / Cleanup Boundary

Desktop uninstall 清理：

- 安装目录。
- Start Menu/Desktop shortcuts。
- uninstall registry entry。
- AgentDash 管理的 startup entry。
- 安装期创建的临时文件。

Runner service uninstall 清理：

- service/unit registration。
- service manager 创建的 unit/script。
- runtime pid/temp files created by service wrapper。

默认保留：

- 用户 workspace。
- 用户任务产物。
- local-runtime data root。
- machine identity。
- profile。
- logs/cache。

如未来提供 purge，必须作为显式可选步骤，不默认执行。

## Manual Acceptance Template

每个验收步骤必须包含：

- Step ID。
- Product。
- Environment。
- Action。
- Expected Result。
- Evidence。
- Diagnostics。
- Gate: block / warn / info。

示例：

| Step ID | Product | Environment | Action | Expected Result | Evidence | Diagnostics | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| WD-01 | Windows Desktop | clean Windows x64 VM | Run NSIS installer | Install succeeds and app entry exists | screenshot, install path | installer log/Event Viewer | block |
| WD-02 | Windows Desktop | installed app | Launch app | Desktop API health ready; Dashboard renders | `/api/health`, screenshot | desktop logs | block |
| LR-01 | Linux Runner | systemd host | install/start service | service active and cloud online | `systemctl status`, cloud backend row | `journalctl` | block |
| WR-01 | Windows Runner | admin PowerShell | install/start service | service Running and cloud online | `Get-Service`, cloud backend row | Event Log/log path | block |

## Release Gate

发布阻断条件：

- release build 不产出 expected files。
- version evidence 不一致。
- installer 无法安装或卸载。
- Desktop API 不可达或绑定非 loopback。
- runner 无法 online。
- service uninstall 后仍残留 running process/service registration。
- 断网重连失败。
- diagnostics/logs 无法定位失败。
- checklist 不能由非实现者执行。

## Handoff Dependencies

From `windows-desktop-installer-background`：

- setup exe output path。
- installed app exe path/process name。
- installer metadata。
- login autostart registry/startup location。
- uninstall cleanup boundary。

From `local-runner-daemon`：

- Linux/Windows release binary build command。
- service names。
- service install/start/status/stop/uninstall commands。
- config/state/log/status paths。
- version command。
- reconnect behavior。

From `runtime-diagnostics-settings`：

- UI paths。
- diagnostic commands。
- logs redaction evidence。
- recovery steps。
