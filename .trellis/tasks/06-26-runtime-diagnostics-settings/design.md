# 运行状态诊断与设置体验 - Design

## Architecture

诊断体验聚合四类状态源：

- Cloud API：用户访问的云端/桌面 Dashboard API 是否可用。
- Desktop API：Tauri 桌面壳内置 Dashboard API 是否启动。
- Local Runtime / Runner：本机执行器是否已配置、运行、在线。
- Relay：WebSocket 是否连接云端，最近断线/重连状态。

前端展示这些状态时必须保留来源标签，避免把 Desktop API 与 Local Runner 混成“本机 API”。

## UI Surface

设置页新增本机运行状态区域：

- 当前 backend id / runner name。
- 注册来源：desktop access token 或 runner registration token。
- 连接目标：server origin、relay ws URL 的脱敏展示。
- 最近错误、日志 tail、重启入口。
- 自启动、启动到托盘、启动后自动连接 runtime。

状态文案以恢复动作为中心：用户能知道要重启 runtime、检查云端连接、重新领取 runner，还是打开桌面 App。

## Contracts

前端优先消费结构化 snapshot，而不是从日志文本推断状态。Snapshot 字段需要区分：

- `desktop_api`
- `local_runtime`
- `runner_registration`
- `relay_connection`

日志展示和导出必须经过脱敏层。

## Tradeoffs

- 第一版聚焦设置页与诊断入口，不把状态信息扩散到多个页面。
- 用结构化状态减少前端靠字符串判断故障类型的风险。
