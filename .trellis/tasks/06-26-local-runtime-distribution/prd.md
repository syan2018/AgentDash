# 安装与本机运行形态产品化

## Goal

将 AgentDash 的本机运行能力产品化为两种清晰交付形态：Windows 桌面完整安装包与 Linux/Windows 独立 Local Runner。用户在桌面场景中应获得可安装、可后台运行、可自启动的完整 App；服务器托管场景中应获得无 UI、出站 WebSocket、可作为系统服务常驻的 runner。

## Requirements

- 统一术语与边界：
  - `Desktop API` 仅属于桌面完整安装包，用于桌面内置前端访问 Dashboard API，默认绑定 `127.0.0.1`。
  - `Local Runner` 指无 UI 本机执行器，不监听业务 API，不开放入站业务端口，第一版只通过 WebSocket 出站连接云端。
- Windows 桌面完整安装包包含 Tauri 桌面壳、内置前端、Desktop API、Local Runtime 生命周期管理、托盘、后台运行、自启动设置。
- 独立 Local Runner 面向 Linux 与 Windows 服务器托管场景，支持配置文件、环境变量、CLI 参数、日志路径、状态输出与系统服务安装/卸载/启动/停止。
- 独立 Local Runner 使用云端生成的 runner registration token 完成无 UI 领取流程，领取后获得 `backend_id`、`relay_ws_url`、`auth_token` 并连接云端。
- 云端保留桌面端基于用户 access token 的 runtime 领取路径，同时新增 runner registration token 的授权路径。
- 前端/桌面设置需要区分云端 API、Desktop API、Local Runtime/Runner、WebSocket relay 的状态。
- 任务采用父任务 + 子任务推进：父任务承载总体架构和集成验收，子任务承载独立可验证交付。

## Acceptance Criteria

- [ ] 父任务包含总体 `prd.md`、`design.md`、`implement.md`，明确交付形态、边界、子任务顺序与集成验收。
- [ ] 子任务覆盖 Local Runner 守护进程、runner 注册令牌、Windows 桌面安装包/后台运行、运行状态诊断设置、发布产物验收。
- [ ] 后续实现中 `Local Runner` 不引入本机业务 HTTP API；如需要健康检查监听端口，必须作为独立设计变更进入规划。
- [ ] Windows 桌面完整安装包验收：全新安装后可启动 UI、连接云端、启动本机 runtime、关闭到后台、自启动、显式退出。
- [ ] Linux runner 验收：仅部署 runner 二进制和配置即可安装为 systemd service，启动后云端能看到 runner 在线并派发任务。
- [ ] Windows runner 验收：仅部署 runner 二进制和配置即可安装为 Windows Service，启动后云端能看到 runner 在线并派发任务。
- [ ] 断网/云端不可达时 runner 自动重连，云端在线状态与本地日志能反映连接变化。
- [ ] 所有日志默认脱敏 token、access token、refresh token。

## Notes

- 第一阶段平台范围：Windows 桌面完整安装包；Linux + Windows 独立 runner。
- 独立 runner 首个目标是服务器守护进程，不是开发者临时 CLI。
- 父任务不直接承载大规模实现；每个子任务在启动前补齐自己的 `design.md` 与 `implement.md`。
