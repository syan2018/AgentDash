# Local Runner 一键部署 CLI helper

## Goal

让服务器托管场景中的 Local Runner 从“手工拼配置 + claim + 安装 service + 查 status”收敛为一条可复制、可交互、可诊断的部署路径。目标用户可以在 Linux 或 Windows 服务器上运行 `agentdash-local setup`，按提示完成注册、配置写入、服务安装、启动和上线验证。

## User Value

- 新机器部署 runner 时不需要理解 `run/status/service` 的底层组合顺序。
- 云端 UI 生成 registration token 后，可以直接给用户一条复制命令或一个交互式 setup 流程。
- 运维排障时可以用稳定的 `doctor` / setup summary 查看配置、service、claim、relay 状态，且不泄露 token。
- 发行团队可以按部署环境给 runner binary 内嵌非密钥默认值，减少用户手动输入 server URL。

## Confirmed Facts

- `agentdash-local` 已有 CLI 子命令：`run`、`status`、`service install/uninstall/start/stop/status/run`、`machine-identity`。
- `runner_config` 已有配置来源优先级：CLI > environment > config file > default。
- 默认配置路径已存在：
  - Windows: `%ProgramData%\AgentDash\runner\config.toml`
  - Linux: `/etc/agentdash/runner.toml`
- 默认状态与日志路径已存在：
  - Windows: `%ProgramData%\AgentDash\runner`
  - Linux: `/var/lib/agentdash/runner` 与 `/var/log/agentdash/runner.log`
- `run` 已能用 registration token 调用 `/api/local-runtime/runner/claim` 并将 `backend_id`、`relay_ws_url`、`auth_token` 写回 config。
- `service` 已能生成 Linux systemd unit / Windows SCM service，并支持 `--dry-run` 与 `--json`。
- `status` 已能合并 config、service 与 runner snapshot，并脱敏输出 token-bearing 字段。

## Requirements

- 新增 `agentdash-local setup` 命令，作为部署 helper，而不是替代现有 `run/status/service`。
- `setup` 支持非交互与交互两种模式：
  - 非交互：通过参数完成部署，适合云端复制命令、脚本和 CI。
  - 交互：缺少关键参数时逐项提示，适合人工服务器登录后部署。
- `setup` 至少收集这些输入：
  - `server_url`
  - `registration_token`
  - `runner_name`
  - `workspace_roots`
  - 是否启用 executor
  - 是否安装 OS service
  - 是否安装后立即启动 service
- `setup` 完成这些动作：
  - 解析默认路径和现有 config。
  - 写入非密钥 runner 配置和 registration token。
  - 执行 claim 并将 server-issued relay credentials 写回 config。
  - 按选项安装 service。
  - 按选项启动 service。
  - 输出脱敏部署摘要。
- `setup --dry-run` 输出计划，不写 config、不 claim、不安装 service、不启动 service。
- `setup --json` 输出稳定 JSON summary，适合云端 UI、安装脚本和测试读取。
- 新增 `agentdash-local doctor` 或等价诊断命令，检查 config、credentials、service、status snapshot、server health、日志路径可写性，并脱敏输出。
- 支持 build-time embedded defaults：
  - 只允许内嵌非密钥默认值，例如 default server URL、默认 runner name prefix、默认 workspace root suggestion。
  - 不允许内嵌 registration token、relay auth token、backend id、access token、refresh token。
  - 内嵌默认值的优先级低于 CLI、environment、config file。
- 云端生成 runner registration token 的 UI 后续可以展示一条完整复制命令，直接调用 `agentdash-local setup`。

## Acceptance Criteria

- [ ] `agentdash-local setup --server-url <url> --registration-token <token> --runner-name <name> --workspace-root <path> --install-service --start` 能完成 config 写入、claim、service install、service start，并输出脱敏 summary。
- [ ] 缺少关键字段时，`agentdash-local setup` 进入交互式提示；在非 TTY 或 `--non-interactive` 模式下缺字段会返回清晰错误。
- [ ] `setup --dry-run` 不产生 OS mutation、不写 token、不调用 claim endpoint，输出将执行的配置路径、service 操作和缺失项。
- [ ] `setup --json` 的输出包含 `config_path`、`server_url`、`runner_name`、`backend_id`、`service_state`、`claim_state`、`relay_state`、`log_path`、`status_path`，并脱敏所有 token。
- [ ] `doctor` 能检查 config、credentials、service、status freshness、log path、server health，并给出 human / JSON 两种输出。
- [ ] Build-time embedded defaults 能通过构建环境变量或 build script 注入，且不会覆盖 CLI/env/config。
- [ ] 单元测试覆盖 CLI 解析、交互输入计划、dry-run、config 写入计划、embedded default precedence、summary redaction。
- [ ] Linux 手工验收：全新服务器运行一条 setup 命令后，systemd service installed/running，云端能看到 runner online。
- [ ] Windows 手工验收：管理员 PowerShell 运行一条 setup 命令后，SCM service installed/running，云端能看到 runner online。

## Out Of Scope

- 不新增本机 HTTP health server；诊断通过 CLI/status/log/cloud online state 完成。
- 不改变 runner registration token 的云端模型。
- 不把 registration token 或 relay auth token 编进 binary。
- 不把 `setup` 做成 GUI installer；第一阶段只做 CLI。

## Open Questions

- 默认发行策略：官方通用 runner binary 是否内嵌 production cloud origin，还是只允许客户/环境专用 binary 内嵌 default server URL？
