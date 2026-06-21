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
- UI 可以保留一键开始，但应建模为组合 intent，而不是让 create API 隐含 drain side effect。

## Pending Product Decisions

- Terminal 是否确认归为 mount utility。
- UI 一键开始是否需要保留，以及由 frontend 组合调用还是 backend 提供组合 command。

