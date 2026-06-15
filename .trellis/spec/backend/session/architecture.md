# Session Architecture

## Role

Session 子系统把来源请求转换为可执行 turn，维护 runtime event、runtime projection、connector input 和终态副作用。目标语义上，当前 `Session` 是 `RuntimeSession`：它只拥有 turn / tool / event / resume / debug / projection / trace lineage，不拥有业务归属、permission scope、Lifecycle progress 或 Agent effective surface。

## Invariants

- 当前启动主线仍是：

```text
LaunchCommand
  -> FrameLaunchEnvelope
  -> LaunchPlan
  -> PreparedTurn
  -> ConnectorAcceptedTurn
  -> CommittedTurn
  -> AttachedTurn
```

- `LaunchCommand` 只表达来源意图；不携带最终 VFS、MCP、capability、context 或 connector facts。
- `FrameLaunchEnvelope` 是 frame construction 到 launch planner 的唯一传递物，必须在 launch 前携带 working directory、VFS、MCP、capability、context、identity 与 resolution trace。
- `LaunchPlan` 只承载单轮启动决策：resolved prompt、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input projection。
- `PreparedTurn` 承载 connector accepted 前准备好的 turn runtime、tools、context frame 与 `ExecutionContext` projection。
- `ConnectorAcceptedTurn` 表达 `connector.prompt` 已返回 `ExecutionStream` 的 accepted 边界。
- `CommittedTurn` 表达 user/start/context/capability/meta/runtime-command/title 等 accepted 后事实已提交。
- `AttachedTurn` 表达 stream 已接入 `SessionTurnProcessor` 与 stream adapter supervision。
- `ExecutionContext` 是 connector-facing projection，不是 application 层事实源。
- 目标控制面中，`AgentFrame` 是 capability / context / VFS / MCP 的事实源；runtime trace/delivery refs 由 `RuntimeSessionExecutionAnchor` 索引和投影。`FrameConstructionService` 负责从 control-plane facts 与 composer 输出生成 `AgentFrame` revision 和 `FrameLaunchEnvelope`。
- `RuntimeSession` 是 delivery / trace substrate。AgentRun delivery/control commands 使用 AgentRun Workspace public identity，accepted result 返回 runtime session / turn / frame refs，原因是用户动作目标是 AgentRun workspace；RuntimeSession 负责 trace refs、event log、connector continuation 与 repository rehydrate。
- AgentRun workspace 的 message intake、queued work、steering continuation 和 system/hook pending work 统一进入 AgentRun Mailbox；scheduler 再映射到 Codex-compatible `turn/start`、`turn/steer` 或 AgentDash envelope extension。原因是 command 幂等、恢复、hook replay dedup 和前端投影需要同一个 durable control-plane 事实源。
- 显式业务资源管理仍从 `ExecutionIntent`、`SubjectRef`、run/agent/frame refs 或 graph instance refs 开始；Lifecycle 内 AgentRun 资源管理语境使用 `/lifecycles/{lifecycle_run_id}/agent-runs`。
- runtime trace 回调以 `RuntimeSessionExecutionAnchor` 建立 delivery evidence，再投影为 run / agent / frame / orchestration node coordinate 进入业务校验；这样 terminal effect、artifact 写入和 node projection 消费同一组 Lifecycle control-plane facts。
- runtime map、active turn、connector live session 是三个不同问题，不能用一个状态互相推断。
- terminal fact 先持久化为事件，业务副作用进入 durable outbox；副作用失败不回滚 terminal event。
- pending runtime delivery command 只保存投递指令；`AgentFrameTransitionRecord` 保存可 replay 的 frame surface transition records，不保存完整 `CapabilityState` projection。

## Current Baseline

核心文档分工：

| 文档 | 当前职责 |
| --- | --- |
| `session-startup-pipeline.md` | LaunchCommand / FrameConstructionService / LaunchPlan / launch stages 主线契约 |
| `runtime-execution-state.md` | runtime registry、turn supervisor、terminal effect、runtime command store |
| `execution-context-frames.md` | connector-facing `ExecutionContext` frame 投影 |
| `bundle-main-datasource.md` | `SessionContextBundle` 主数据面 |
| `streaming-protocol.md` | NDJSON session stream wire contract |
| `pi-agent-streaming.md` | PiAgent `AgentEvent -> BackboneEnvelope` 映射 |
| `context-compaction-projection.md` | compact checkpoint、projection store、ContextProjector 与模型上下文查询契约 |
| `session-lineage-projection.md` | session lineage、fork、rollback 与 branch-aware restore 契约 |

## Local Decisions

- Frame construction 阶段一次性产出 launch-ready final facts，原因是 context query、inspector、audit 和 connector launch 必须观察同一份事实。
- Project / Story / Routine owner bootstrap composition 归 `workflow::frame_construction`，原因是 owner VFS、capability、MCP、context bundle 与 execution profile 的组合结果会写入 `AgentFrame` surface；session 层只消费 `FrameLaunchEnvelope` 进入 launch / delivery / trace。
- runtime delivery replay 从 frame runtime surface 开始，并从 `AgentFrameTransitionRecord` 投影出 capability transition，原因是 pending transition、context query 和 next-turn launch 必须共享相同闭包逻辑。
- terminal effect 使用 outbox，原因是业务副作用需要跨进程恢复，且不应影响 terminal event 的事实性。
- 会话标题由 `TitleSource` 管控：用户手动标题优先，其次接受具备来源标题能力的 connector 通过 typed Backbone event 提供的标题；无来源标题能力时才从首条用户消息本地派生 `auto` 标题。原因是标题属于会话列表元信息，业务层不应绑定 provider 私有实现，也不应为标题额外消耗模型执行能力。
- 上下文压缩采用 Codex-aligned lifecycle 加 AgentDash-owned projection store。原因是 compact 在产品上是可观察 lifecycle，在恢复上是模型上下文 checkpoint；二者分层后，timeline、ContextFrame、agent input、branch restore 可以共享 durable facts 但消费不同 projection。
- fork 默认把 parent fork point 的模型可见 projection 固化为 child session 自己的 initial compaction。原因是 child 的继续执行、retention、rollback 和团队协作权限都应依赖 child 自身的 durable facts，而不是重新读取 parent 的 live projection。
- `RuntimeSessionExecutionAnchor` 承载 session 到 lifecycle control-plane identity 的反查，原因是 `RuntimeSession` 是 trace substrate，而业务推进需要稳定落到 run、agent、frame、assignment 和 activity attempt。
- Task terminal effect 的校验先从 trace callback 解析 `RuntimeSessionExecutionAnchor`，再构造 `run_id + agent_id + frame_id + orchestration_id + node_path + attempt` coordinate，原因是 artifact/status side effect 需要绑定明确的 runtime node evidence。
- 用户输入在 session 链路只有单一 canonical 表示 `UserInputBlock`（`agentdash-agent-protocol` 对 Codex app-server v2 `UserInput` 的封名别名），贯穿 API 入参 → `UserPromptInput.input` → `PromptPayload::Input` → connector。连接器边界用唯一映射 `user_input_blocks_to_content_parts` 转 `Vec<ContentPart>`：图片（data URL / 可读 `LocalImage`）直达 `ContentPart::Image`，`Skill`/`Mention` 收敛为定义集中一处的文本语义。原因是历史上 prompt / steer / continuation 三路各自把输入拍平成文本（图片因此丢失多模态），且 ACP `ContentBlock` / codex `UserInput` / `ContentPart` 三套表示并存产生 ≥4 个平行 flattener；收敛为单表示 + 单映射后，多模态可结构化直达模型，且后续替换为自定义扩展类型只需改别名与映射单点。`ContentBlock` 仅保留在 relay 远程边界的单处双向转换，`codex_user_input_to_text` 仅作标题 / trace 摘要、非投递路径。
- AgentRun lifecycle naming uses `AgentRunThread` for workspace-level thread, `AgentRunTurn` for the user-visible `start_prompt -> terminal` execution, and `AgentLoopTurn` only for PiAgent/agent loop `AgentEvent::TurnStart/TurnEnd` boundaries referenced by mailbox scheduling. This keeps public control-plane language aligned with Codex `Thread/Turn` while avoiding ambiguity with internal loop turns.

## Contract Appendices

- [Session Startup Pipeline](./session-startup-pipeline.md)
- [Session Runtime Execution State](./runtime-execution-state.md)
- [AgentRun Mailbox And Turn Boundary Contract](./agentrun-mailbox.md)
- [Execution Context Frames](./execution-context-frames.md)
- [Session Context Bundle](./bundle-main-datasource.md)
- [NDJSON Streaming Protocol](./streaming-protocol.md)
- [Pi Agent Streaming](./pi-agent-streaming.md)
- [Context Compaction Projection](./context-compaction-projection.md)
- [Session Lineage Projection](./session-lineage-projection.md)
