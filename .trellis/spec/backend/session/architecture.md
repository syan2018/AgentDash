# Session Architecture

## Role

Session 子系统把来源请求转换为可执行 turn，维护 session event、runtime projection、connector input 和终态副作用。它的职责是让所有 session 启动、续跑、context 查询和 runtime transition 走同一条可审计主线。

## Invariants

- Session 启动主线是：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchPlan
  -> PreparedTurn
  -> ConnectorAcceptedTurn
  -> CommittedTurn
  -> AttachedTurn
```

- `LaunchCommand` 只表达来源意图；不携带最终 VFS、MCP、capability、context 或 connector facts。
- `SessionConstructionPlan` 是构建事实源，必须在 launch 前产出 owner、workspace、working directory、VFS、MCP、capability、context、identity 与 resolution trace。
- `LaunchPlan` 只承载单轮启动决策：resolved prompt、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input projection。
- `PreparedTurn` 承载 connector accepted 前准备好的 turn runtime、tools、context frame 与 `ExecutionContext` projection。
- `ConnectorAcceptedTurn` 表达 `connector.prompt` 已返回 `ExecutionStream` 的 accepted 边界。
- `CommittedTurn` 表达 user/start/context/capability/meta/runtime-command/title 等 accepted 后事实已提交。
- `AttachedTurn` 表达 stream 已接入 `SessionTurnProcessor` 与 stream adapter supervision。
- `ExecutionContext` 是 connector-facing projection，不是 application 层事实源。
- runtime map、active turn、connector live session 是三个不同问题，不能用一个状态互相推断。
- terminal fact 先持久化为事件，业务副作用进入 durable outbox；副作用失败不回滚 terminal event。
- pending runtime command 保存可 replay transition records，不保存完整 `CapabilityState` projection。

## Current Baseline

核心文档分工：

| 文档 | 当前职责 |
| --- | --- |
| `session-startup-pipeline.md` | LaunchCommand / Construction / LaunchPlan / launch stages 主线契约 |
| `runtime-execution-state.md` | runtime registry、turn supervisor、terminal effect、runtime command store |
| `execution-context-frames.md` | connector-facing `ExecutionContext` frame 投影 |
| `bundle-main-datasource.md` | `SessionContextBundle` 主数据面 |
| `streaming-protocol.md` | NDJSON session stream wire contract |
| `pi-agent-streaming.md` | PiAgent `AgentEvent -> BackboneEnvelope` 映射 |

## Local Decisions

- Construction 阶段一次性产出 launch-ready final facts，原因是 context query、inspector、audit 和 connector launch 必须观察同一份事实。
- runtime command replay 从 construction base projection 开始，原因是 pending transition、context query 和 next-turn launch 必须共享相同闭包逻辑。
- terminal effect 使用 outbox，原因是业务副作用需要跨进程恢复，且不应影响 terminal event 的事实性。

## Contract Appendices

- [Session Startup Pipeline](./session-startup-pipeline.md)
- [Session Runtime Execution State](./runtime-execution-state.md)
- [Execution Context Frames](./execution-context-frames.md)
- [Session Context Bundle](./bundle-main-datasource.md)
- [NDJSON Streaming Protocol](./streaming-protocol.md)
- [Pi Agent Streaming](./pi-agent-streaming.md)
