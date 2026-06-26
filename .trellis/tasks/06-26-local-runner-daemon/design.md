# Local Runner 服务器守护进程交付 - Design

## Architecture

`agentdash-local` 成为独立 Local Runner 的 headless 产品入口。Runner 只负责本机能力执行与云端 relay 出站连接，不启动 Dashboard API，不承载桌面 UI，也不提供业务 HTTP API。

第一版平台范围：

- Linux：安装为 systemd service，服务名 `agentdash-local-runner`。
- Windows：安装为 Windows Service，服务名 `AgentDashLocalRunner`。

## Configuration

配置来源优先级固定为：CLI 参数 > 环境变量 > 配置文件。默认配置路径：

- Linux：`/etc/agentdash/runner.toml`
- Windows：`%PROGRAMDATA%\AgentDash\runner\config.toml`

核心配置项：

- `server_url`：云端 HTTP origin，用于领取 runner 凭据。
- `registration_token`：云端生成的 runner registration token，仅用于首次领取或轮换。
- `backend_id`、`relay_ws_url`、`auth_token`：领取后保存的运行凭据。
- `name`、`workspace_roots`、`executor_enabled`、`log_path`。

Runner 启动时如果已有运行凭据，直接连接 `relay_ws_url`；如果缺少运行凭据但存在 `registration_token`，先执行领取流程并写回配置。

## Service Model

服务管理命令由 `agentdash-local service <action>` 提供：

- `install`：生成并安装 systemd unit 或 Windows Service。
- `uninstall`：停止并移除服务。
- `start` / `stop`：调用平台服务管理器。
- `status`：读取平台服务状态，并叠加 runner 最近连接状态。

服务安装不会写入用户 access token。Runner 长期凭据来自 registration token 领取结果，并通过文件权限保护。

## Diagnostics

日志默认写入：

- Linux：`/var/log/agentdash/runner.log`
- Windows：`%PROGRAMDATA%\AgentDash\runner\runner.log`

日志与状态输出必须脱敏 token、access token、refresh token、auth_token。`status` 输出用于本机运维，云端在线状态仍以 relay registry 为事实源。

## Tradeoffs

- 不引入 HTTP 健康检查端口，避免服务器托管场景暴露入站面。
- 第一版直接提供服务安装命令，让 runner 交付成为可运维产物，而不是只给二进制和示例脚本。
