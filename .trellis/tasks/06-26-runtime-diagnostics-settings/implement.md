# 运行状态诊断与设置体验 - Implement

## Checklist

- 后端/桌面状态快照：
  - 补齐 Desktop API、Local Runtime/Runner、relay 连接状态字段。
  - 暴露最近错误、日志 tail、重启命令。
  - 确保 token 类字段输出前脱敏。
- 前端服务层：
  - 增加结构化类型与 mapper。
  - 明确注册来源枚举：desktop access token、runner registration token。
- 设置页 UI：
  - 增加本机运行状态区域。
  - 提供重启 runtime/runner、查看日志、清空日志入口。
  - 提供自启动、启动到托盘、启动后自动连接 runtime 设置。
- 文案：
  - 云端 API、Desktop API、Local Runtime/Runner、relay 分别给出故障说明。

## Validation

- `pnpm run frontend:check`
- `pnpm run frontend:lint`
- 相关前端测试覆盖状态 mapper 与异常展示。
- 桌面手工验收状态刷新、日志展示、重启入口。

## Risk Points

- 前端不能从日志字符串推断状态。
- 日志 tail 与诊断导出不能泄露 token。
- 桌面设置只管理桌面生命周期，不管理独立 runner 的系统服务生命周期。
