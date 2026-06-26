# 运行状态诊断与设置体验

## Goal

让用户能清楚理解 AgentDash 本机运行链路的状态，区分云端 API、Desktop API、Local Runtime/Runner 与 WebSocket relay，并在出现问题时获得可恢复入口。

## Requirements

- 前端/桌面设置展示云端 API 状态、Desktop API 状态、Local Runtime/Runner 在线状态、WebSocket relay 连接状态。
- 展示当前 runner/backend id、注册来源、最近连接错误、日志 tail、重启入口。
- 注册来源至少区分桌面 access token 与 runner registration token。
- 提供自启动、启动到托盘、启动后自动连接 runtime 等桌面设置入口。
- 诊断日志默认脱敏 token、access token、refresh token。
- 错误文案面向用户解释当前是哪一层不可用。

## Acceptance Criteria

- [ ] 用户能从设置页判断是云端 API、Desktop API、Local Runtime/Runner 还是 relay 连接异常。
- [ ] 用户能查看最近日志并触发 runtime/runner 重启。
- [ ] 注册来源、backend id、连接目标展示准确。
- [ ] 日志和诊断导出不泄露 token 类字段。
- [ ] 前端类型检查、lint 和相关测试通过。

## Notes

- 本任务依赖 runner 与桌面端提供稳定状态快照。
- 启动前补齐本子任务 `design.md` 与 `implement.md`。
