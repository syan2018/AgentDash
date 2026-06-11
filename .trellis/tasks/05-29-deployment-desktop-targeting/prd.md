# 桌面端服务器指向与发布策略

## Goal

让桌面端能正确指向已部署的 AgentDash 服务器，并形成通用包与预配置包两类发布策略。

## Parent Task

- `.trellis/tasks/05-29-deployment-baseline`
- 主文档：`docs/deployment/deployment-baseline.md`

## Assigned Workstream

建议由其它机器上的桌面/前端 Agent 处理。该子任务优先碰 Tauri 桌面端、desktop build 脚本、服务器配置持久化、discovery/compatibility 检查和发布说明，不负责云端部署脚本。

## Requirements

- 梳理当前 `desktop:build` / `desktop:bundle` 的 API mode 语义。
- 明确 `builtin`、`external`、`sidecar` 在发布语义中的定位。
- 将构建时 default API origin 定义为默认值，而不是唯一服务器地址。
- 设计或实现运行时服务器地址配置：
  - 首次启动可输入服务器地址。
  - 可读取构建时默认服务器。
  - 可持久化用户选择。
  - 可在设置中切换或重置。
- 接入或预留 `/.well-known/agentdash` discovery。
- 设计或实现 desktop compatibility check：
  - 低于最低版本时阻止连接。
  - 低于推荐版本时提示升级。
  - 服务器不可达时保留配置并显示失败状态。
- 明确通用包和预配置包的发布命令。

## Acceptance Criteria

- [ ] 文档明确桌面端如何指向目标服务器。
- [ ] 文档明确通用包与预配置包发布方式。
- [ ] 有清晰的运行时服务器配置状态流。
- [ ] 有清晰的 discovery / compatibility 数据流。
- [ ] 不实现云端 discovery endpoint 本身。
- [ ] 不创建 Compose 部署文件。

## Dependencies

- 依赖云端发布原语子任务确定 discovery endpoint 字段。
- 如果 discovery endpoint 尚未实现，可先基于父任务文档中的目标 schema 做 UI / 配置设计。

## Open Questions

- 服务器地址配置应该放在 Tauri native config 还是 Web app 设置层。
- 桌面端是否需要在未连接服务器时仍展示本机 runtime 管理能力。
- 桌面自动更新是否完全后置，仅保留版本提示。
