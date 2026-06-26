# Local Runner 服务器守护进程交付 - Implement

## Checklist

- 扩展 `agentdash-local` CLI：
  - 新增 `--config`、配置文件加载、环境变量加载。
  - 新增 `service install|uninstall|start|stop|status` 子命令。
  - 新增状态输出命令，展示配置来源、backend id、relay 目标、最近连接状态、日志路径。
- 实现 runner 配置模型：
  - `RunnerConfig` 表达配置文件与环境变量。
  - `ResolvedRunnerConfig` 表达最终运行配置。
  - CLI > env > file 的合并规则必须有单元测试。
- 接入 registration token 领取：
  - 缺少运行凭据时调用云端 runner claim API。
  - 领取成功后写回 `backend_id`、`relay_ws_url`、`auth_token`。
  - 领取失败保留明确错误，不启动半配置 runner。
- 实现服务安装：
  - Linux 生成 systemd unit，包含配置路径、工作目录、日志约定。
  - Windows 注册 `AgentDashLocalRunner` 服务，启动参数包含配置路径。
  - `status` 同时报告平台服务状态与 runner 最近连接状态。
- 保持 WebSocket 主循环：
  - 复用现有 `LocalRuntimeConfig` / `run_standalone` 连接逻辑。
  - 保留断线重连。
  - 不新增入站业务 HTTP API。

## Validation

- `cargo test -p agentdash-local`
- 覆盖配置合并、缺失配置、registration token claim 失败、日志脱敏、服务命令生成。
- Linux 手工验收：安装 service、启动、停止、卸载、断网重连。
- Windows 手工验收：安装 service、启动、停止、卸载、断网重连。

## Risk Points

- Windows Service 运行用户与工作区访问权限必须在设计里明确，不默认继承桌面用户权限。
- 配置文件写回必须原子化，避免领取成功后进程崩溃造成半写入。
- 日志中不能输出 registration token 或 auth token。
