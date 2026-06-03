# Session Architecture

## Role

Session 子系统把来源请求转换为可执行 turn，维护 runtime event、runtime projection、connector input 和终态副作用。目标语义上，当前 `Session` 是 `RuntimeSession`：它只拥有 turn / tool / event / resume / debug / projection / trace lineage，不拥有业务归属、permission scope、Lifecycle progress 或 Agent effective surface。

## Invariants

- 当前启动主线仍是：

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
- 目标控制面中，`AgentFrame` 是 capability / context / VFS / MCP 的事实源；runtime trace/delivery refs 由 `RuntimeSessionExecutionAnchor` 索引和投影。`SessionConstructionPlan` 与 `LaunchPlan` 将降为 frame builder / runtime adapter 的内部结构。
- `RuntimeSession` 只能作为 delivery / trace substrate。业务 command path 必须从 `ExecutionIntent`、`SubjectRef`、run/agent/frame refs 或 graph instance refs 开始。
- 通过 runtime session 反查业务上下文时，走 `RuntimeSessionExecutionAnchor -> AgentFrame -> LifecycleAgent -> LifecycleRun -> LifecycleSubjectAssociation`；没有 anchor 的 runtime trace 只能作为不可继续发送的消息壳展示。
- runtime map、active turn、connector live session 是三个不同问题，不能用一个状态互相推断。
- terminal fact 先持久化为事件，业务副作用进入 durable outbox；副作用失败不回滚 terminal event。
- pending runtime delivery command 只保存投递指令；`AgentFrameTransitionRecord` 保存可 replay 的 frame surface transition records，不保存完整 `CapabilityState` projection。

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
| `context-compaction-projection.md` | compact checkpoint、projection store、ContextProjector 与模型上下文查询契约 |
| `session-lineage-projection.md` | session lineage、fork、rollback 与 branch-aware restore 契约 |

## Local Decisions

- Construction 阶段一次性产出 launch-ready final facts，原因是 context query、inspector、audit 和 connector launch 必须观察同一份事实。
- runtime delivery replay 从 construction base projection 开始，并从 `AgentFrameTransitionRecord` 投影出 capability transition，原因是 pending transition、context query 和 next-turn launch 必须共享相同闭包逻辑。
- terminal effect 使用 outbox，原因是业务副作用需要跨进程恢复，且不应影响 terminal event 的事实性。
- 会话标题由 `TitleSource` 管控：用户手动标题优先，其次接受具备来源标题能力的 connector 通过 typed Backbone event 提供的标题；无来源标题能力时才从首条用户消息本地派生 `auto` 标题。原因是标题属于会话列表元信息，业务层不应绑定 provider 私有实现，也不应为标题额外消耗模型执行能力。
- 上下文压缩采用 Codex-aligned lifecycle 加 AgentDash-owned projection store。原因是 compact 在产品上是可观察 lifecycle，在恢复上是模型上下文 checkpoint；二者分层后，timeline、ContextFrame、agent input、branch restore 可以共享 durable facts 但消费不同 projection。
- fork 默认把 parent fork point 的模型可见 projection 固化为 child session 自己的 initial compaction。原因是 child 的继续执行、retention、rollback 和团队协作权限都应依赖 child 自身的 durable facts，而不是重新读取 parent 的 live projection。
- `RuntimeSessionExecutionAnchor` 承载 session 到 lifecycle control-plane identity 的反查，原因是 `RuntimeSession` 是 trace substrate，而业务推进需要稳定落到 run、agent、frame、assignment 和 activity attempt。

## Contract Appendices

- [Session Startup Pipeline](./session-startup-pipeline.md)
- [Session Runtime Execution State](./runtime-execution-state.md)
- [Execution Context Frames](./execution-context-frames.md)
- [Session Context Bundle](./bundle-main-datasource.md)
- [NDJSON Streaming Protocol](./streaming-protocol.md)
- [Pi Agent Streaming](./pi-agent-streaming.md)
- [Context Compaction Projection](./context-compaction-projection.md)
- [Session Lineage Projection](./session-lineage-projection.md)
