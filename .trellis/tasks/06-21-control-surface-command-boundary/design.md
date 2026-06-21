# Control Surface Design

## Problem Shape

控制面当前有多种 command 入口各自选择 target 或 side effect：Lifecycle start API 同时 create + drain，ConversationSnapshot 被 route policy 复用为校验工具，Extension panel 和 WorkspaceModule action 的 backend target resolver 不一致，Terminal 与 execution lease 的关系未明确。

## Command Taxonomy

| Category | Meaning | Examples | Target Owner |
| --- | --- | --- | --- |
| execution-placement-bound | 绑定当前执行链路和 execution lease | prompt, cancel, AgentRun mailbox commands | Runtime Coordinate selection |
| session-route-bound | 绑定具体 runtime session route | session continuation, session MCP during execution | Session route / delivery binding |
| mount-utility-bound | 绑定 workspace mount / backend utility | VFS file tool, terminal if confirmed as utility | VFS / workspace mount resolver |
| setup-bound | 运行前配置、probe、discovery | MCP probe, backend setup | setup resolver / catalog |

## Lifecycle Command Shape

- `create lifecycle run` 只创建 Ready run。
- `continue/drain ready nodes` 显式推进 Ready nodes。
- UI 可以保留一键开始，但必须进入后端显式组合 command；组合 command 语义是 create Ready run + continue/drain，不复用 `POST /lifecycle-runs` 的隐式 side effect。

## Terminal Completion Protocol

- Terminal 归属 `mount-utility-bound`，绑定 workspace mount/backend，不占 AgentRun execution lease。
- Terminal completion 是 mount utility event，但可以成为 AgentRun 输入信号。
- canonical 路径是写入可恢复 outbox，再由 AgentRun mailbox 消费为 steer 或 turn-boundary 调度。
- hook 可以参与 Terminal completion 行为，但不能成为唯一默认协议。

## Extension Target Resolver

- session-bound extension action/channel 只能使用当前 session 关联 backend，不能 fallback 到 Project workspace binding 或任意在线 backend。
- API / panel / iframe 只表达 action/channel intent 和 input；backend target 由宿主/API 后端 resolver 组装。
- Project-level 非 session invocation 是后续能力，本轮只保留 contract/design 扩展点，不实现 fallback。

## Command Availability Resolver

- ConversationSnapshot 与 workspace route policy 共用 application 层 command availability resolver。
- resolver 输入是 AgentRun、frame/runtime、execution state、mailbox pause/visible count、model config status 和 steering support 这些控制面事实；输出 command set、snapshot id 和 stale guard 所需事实。
- route policy 只消费 availability 输出校验 command kind/id、stale guard 和 enabled 状态，原因是 API command admission 不应通过重建完整 browser UI projection 获得命令可用性。
