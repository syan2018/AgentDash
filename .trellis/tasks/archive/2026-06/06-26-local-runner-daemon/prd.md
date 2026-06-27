# Local Runner 服务器守护进程交付

## Goal

将 `agentdash-local` 产品化为独立 Local Runner，服务 Linux 与 Windows 服务器托管场景。Runner 无 UI、长期运行、通过 WebSocket 出站连接云端，并可安装为系统服务。

## Requirements

- Runner 支持配置文件、环境变量、CLI 参数三类配置来源，并在本任务 `design.md` 中固定优先级。
- Runner 支持运行状态输出，至少能显示配置来源、backend id、连接目标、最近连接状态、服务状态和日志路径。
- Runner 支持日志文件输出，日志默认脱敏 token、access token、refresh token。
- Runner 支持服务管理子命令：`service install`、`service uninstall`、`service start`、`service stop`、`service status`。
- Linux 第一版安装为 systemd service。
- Windows 第一版安装为 Windows Service。
- Runner 不启动 Dashboard API，不监听业务 HTTP API，不对外开放入站业务端口。
- Runner 保留现有 WebSocket 断线重连主循环。

## Acceptance Criteria

- [ ] Linux runner 可通过配置文件和 registration token 安装为 systemd service，启动后云端显示 online。
- [ ] Windows runner 可通过配置文件和 registration token 安装为 Windows Service，启动后云端显示 online。
- [ ] 缺少注册信息、云端不可达、无效 token、WebSocket 断线均有明确日志和退出/重试语义。
- [ ] `service status` 能反映系统服务状态与 runner 最近连接状态。
- [ ] Runner 代码路径不引入本机业务 HTTP API。
- [ ] `cargo test -p agentdash-local` 覆盖配置加载、服务命令组装、日志脱敏和错误路径。

## Notes

- 依赖 `06-26-runner-enrollment-token` 提供稳定的 registration token 领取契约。
- 启动前补齐本子任务 `design.md` 与 `implement.md`。
