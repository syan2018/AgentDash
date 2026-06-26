# 安装与本机运行形态产品化 - Implement

## Execution Order

1. 完成 `runner-enrollment-token`：先建立无 UI runner 的云端注册/领取契约，避免 runner 服务化后仍依赖手动传 `backend_id` 或用户 access token。
2. 完成 `local-runner-daemon`：基于注册令牌契约实现 headless runner 配置、日志、状态与 Linux/Windows 服务管理。
3. 完成 `windows-desktop-installer-background`：完善 Windows 桌面安装包、托盘、后台运行和自启动。
4. 完成 `runtime-diagnostics-settings`：把云端 API、Desktop API、Local Runtime/Runner、relay 连接状态呈现给用户。
5. 完成 `distribution-release-validation`：固化构建产物、版本一致性与手工验收流程。

## Planning Gates

- 每个子任务启动前必须补齐该子任务自己的 `design.md` 与 `implement.md`。
- 涉及 API、数据库、协议、前端 DTO 的子任务必须先读 cross-layer spec/guides，并在实现后跑 contracts/check。
- 任何引入 runner 入站监听端口的想法都必须回到父任务设计重新评审。

## Validation Commands

- Backend/API 子任务：`pnpm run contracts:check`、`pnpm run backend:check`、相关 `cargo test`。
- Runner 子任务：`cargo test -p agentdash-local`、必要时补充 Linux/Windows 服务命令 dry-run 或平台专项验证。
- Desktop 子任务：`pnpm run desktop:check`、`pnpm run desktop:bundle`，并进行 Windows 手工安装/卸载验收。
- Frontend/settings 子任务：`pnpm run frontend:check`、`pnpm run frontend:lint`、相关前端测试。
- 集成收口：按父任务 PRD 的 manual acceptance checklist 执行。

## Risk Points

- Runner registration token 的权限范围必须与 backend/project 可见性一致，否则会造成服务器 runner 过度授权。
- Windows Service 与桌面自启动是不同生命周期，不能复用同一个“开机启动”语义。
- Desktop API 默认绑定 `127.0.0.1`；独立 runner 不应因为诊断需求引入业务 HTTP API。
- 日志、错误消息和配置导出必须脱敏 token 类字段。
