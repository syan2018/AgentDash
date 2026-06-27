# Research: agentdash-stable-boundary

- Query: AgentDash 当前 session event store、turn terminal、projection、repository restore、frontend backlog/feed 的稳定边界；失败后丢弃最后一个未稳定轮次应以哪些事件/字段为边界，以及下一次 AgentRun 输入上下文由哪些代码路径重建。
- Scope: internal
- Date: 2026-06-23

## Findings

### Files Found

| Path | Description |
| --- | --- |
| `.trellis/workflow.md` | Trellis planning/research contract；本研究属于 Phase 1.2 research，必须持久化到 task research。 |
| `.trellis/spec/backend/session/session-startup-pipeline.md` | Session launch 主链路；定义 connector accepted 之后才提交 user message、`TurnStarted`、context/capability projection event。 |
| `.trellis/spec/backend/session/runtime-execution-state.md` | Runtime execution、terminal effect、SessionMeta trace metadata 与 AgentRun mailbox 边界。 |
| `.trellis/spec/backend/session/context-compaction-projection.md` | `ContextProjector` 的 durable facts 读取顺序与 projection head/segment 语义。 |
| `.trellis/spec/backend/session/agentrun-mailbox.md` | AgentRun mailbox durable envelope、scheduler、terminal boundary 与 recovery 语义。 |
| `.trellis/spec/backend/session/streaming-protocol.md` | Session NDJSON stream 字段语义：`event_seq`、`session_update_type`、`turn_id`、`entry_index`。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | Rust contract -> generated TS -> frontend service/reducer 的 DTO 契约。 |
| `crates/agentdash-spi/src/session_persistence.rs` | Session persistence trait 和持久化 DTO；定义 `SessionMeta`、`PersistedSessionEvent`、event/projection/effect stores。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` | PostgreSQL `SessionEventStore` 实现；append/read/page/list 与 SessionMeta 投影写回。 |
| `crates/agentdash-infrastructure/src/persistence/session_core.rs` | 从 Backbone envelope 推导 `last_delivery_status`、`last_turn_id`、`entry_index`、`executor_session_id` 等 repository projection。 |
| `crates/agentdash-application/src/session/eventing.rs` | `SessionEventingService`；append/broadcast、context projection read model、compaction projection commit 入口。 |
| `crates/agentdash-application/src/session/launch/orchestrator.rs` | Launch pipeline 编排：claim -> frame construction -> plan -> prepare -> connector accepted -> commit -> stream ingestion。 |
| `crates/agentdash-application/src/session/launch/commit.rs` | Accepted commit 边界；connector accepted 后提交 user input、`TurnStarted`、context frames、runtime command applied、AgentFrame revision。 |
| `crates/agentdash-application/src/session/turn_processor.rs` | Turn stream ingestion 与 terminal event 持久化；terminal event 先落库，再释放 active turn，再 dispatch effects。 |
| `crates/agentdash-application/src/session/hub_support.rs` | 构造 `UserInputSubmitted`、`TurnStarted`、`turn_terminal` platform event，并把 `SessionMeta` 投影成 execution state。 |
| `crates/agentdash-application/src/session/types.rs` | `resolve_prompt_launch_path`；冷启动 repository rehydrate 的判定依据。 |
| `crates/agentdash-application/src/session/context_projector.rs` | 模型上下文 projection 构建；读 projection head/segments 后接 suffix events。 |
| `crates/agentdash-application/src/session/continuation.rs` | 从持久化事件重建 projected transcript；`turn_id + entry_index` 是 assistant message restore key。 |
| `crates/agentdash-executor/src/connectors/pi_agent/connector.rs` | Pi Agent connector repository restore 消费点；新建/重建 agent 时 `replace_messages_with_refs`。 |
| `crates/agentdash-application/src/agent_run/mailbox.rs` | AgentRun mailbox command、policy、schedule、claim/consume、launch/steer、pause_for_terminal 主体。 |
| `crates/agentdash-application/src/agent_run/message_delivery.rs` | Mailbox launch delivery -> `SessionLaunchService::launch_command_in_task`。 |
| `crates/agentdash-application/src/session/mailbox_delegate.rs` | Agent loop boundary / hook delivery 消费 mailbox，并持久化 steer `UserInputSubmitted`。 |
| `crates/agentdash-api/src/agent_run_mailbox.rs` | Terminal callback：completed 调度下一条，failed/interrupted pause mailbox。 |
| `crates/agentdash-application/src/agent_run/workspace/projection.rs` | AgentRun workspace status/read model 从 `SessionExecutionState` 派生。 |
| `crates/agentdash-api/src/routes/sessions.rs` | `/sessions/{id}/events` backlog 与 `/sessions/{id}/stream/ndjson` 实时出口。 |
| `crates/agentdash-contracts/src/runtime/session.rs` | `SessionEventResponse` / `SessionNdjsonEnvelope` wire DTO。 |
| `packages/app-web/src/services/session.ts` | 前端 event page API：`fetchSessionEvents(after_seq, limit)`。 |
| `packages/app-web/src/features/session/model/streamTransport.ts` | 前端 NDJSON parser/reconnect cursor；校验 `event_seq`、`session_update_type` 等字段。 |
| `packages/app-web/src/features/session/model/useSessionStream.ts` | 前端先 hydrate backlog，再以 `lastAppliedSeq` 连接增量 stream。 |
| `packages/app-web/src/features/session/model/sessionStreamReducer.ts` | 前端 rawEvents/display entries 的事实 reducer；按 `event_seq` 排序去重。 |
| `packages/app-web/src/features/session/model/useSessionFeed.ts` | 前端 display feed aggregation 与 turn segmentation。 |
| `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` | AgentRun workspace 使用 workspace projection、mailbox snapshot 和 session feed。 |

### Current Event Store Boundary

`SessionMeta` 保存 runtime trace head：`last_event_seq`、`last_delivery_status`、`last_turn_id`、`last_terminal_message`、`executor_session_id`。这些字段是 repository restore、workspace projection、stream cursor 的核心摘要字段。定义见 `crates/agentdash-spi/src/session_persistence.rs:304`，字段见 `crates/agentdash-spi/src/session_persistence.rs:312`、`crates/agentdash-spi/src/session_persistence.rs:314`、`crates/agentdash-spi/src/session_persistence.rs:316`、`crates/agentdash-spi/src/session_persistence.rs:318`、`crates/agentdash-spi/src/session_persistence.rs:320`。

`PersistedSessionEvent` 是当前 session event store 的持久化单元：`event_seq` 是单调序号，`session_update_type` 是后端归档的事件标签，`turn_id` / `entry_index` / `tool_call_id` 是前端合并和恢复投影坐标，`notification` 是完整 `BackboneEnvelope`。定义见 `crates/agentdash-spi/src/session_persistence.rs:531`。

PostgreSQL `append_event` 的事务边界很重要：

- 先 `UPDATE sessions SET last_event_seq = last_event_seq + 1 RETURNING last_event_seq` 分配新 `event_seq`，见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:330`。
- 用 `projection_from_envelope(envelope)` 得到 `turn_id`、`entry_index`、`tool_call_id`、terminal/meta projection，见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:347`。
- 插入 `session_events`，字段包含 `session_update_type`、`turn_id`、`entry_index`、`tool_call_id`、`notification_json`，见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:362`。
- 在同一事务里更新 `sessions.updated_at`、`last_delivery_status`、`last_turn_id`、`last_terminal_message`、`executor_session_id`，见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:383`。

`read_backlog` 读取的是一个快照窗口：先取 `sessions.last_event_seq` 为 `snapshot_seq`，再读 `event_seq > after_seq AND event_seq <= snapshot_seq`，见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:415` 和 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:423`。这意味着 `event_seq` 是前后端恢复游标，不能轻易重排或复用。

当前 `SessionEventStore` trait 只有 append/read/list，没有 tail truncate、mark ignored、rewrite event 或 rebuild meta 方法，见 `crates/agentdash-spi/src/session_persistence.rs:797`。`save_session_meta` 对 `last_event_seq` 使用 `GREATEST(sessions.last_event_seq, excluded.last_event_seq)`，旧 meta 不能把 head 往回写，见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:216` 和 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:229`。因此“回到上一稳定边界”如果要物理回退，需要新增 repository 能力；只保存一个较小 `last_event_seq` 不会生效。

### Turn Start And Terminal Boundary

当前 accepted boundary 不是 route success，也不是 mailbox envelope 创建，而是 `connector.prompt` 返回 `ExecutionStream` 后进入 accepted commit。`SessionLaunchOrchestrator` 的顺序是：claim prompt、frame construction、planning、preparation、connector start，connector accepted 后 `TurnCommitter::commit`，最后 attach stream ingestion，见 `crates/agentdash-application/src/session/launch/orchestrator.rs:203`、`crates/agentdash-application/src/session/launch/orchestrator.rs:224`、`crates/agentdash-application/src/session/launch/orchestrator.rs:245`、`crates/agentdash-application/src/session/launch/orchestrator.rs:261`、`crates/agentdash-application/src/session/launch/orchestrator.rs:270`。

`TurnCommitter::commit` 是本轮成为 durable facts 的入口。它在 connector accepted 后先提交 `UserInputSubmitted` 和 `TurnStarted`，再提交 context frames、更新 meta、标记 runtime commands applied、写 accepted AgentFrame revision，见 `crates/agentdash-application/src/session/launch/commit.rs:34`、`crates/agentdash-application/src/session/launch/commit.rs:44`、`crates/agentdash-application/src/session/launch/commit.rs:52`、`crates/agentdash-application/src/session/launch/commit.rs:67`、`crates/agentdash-application/src/session/launch/commit.rs:76`、`crates/agentdash-application/src/session/launch/commit.rs:112`。

`commit_accepted_launch_events` 的顺序是：如果本轮有用户输入，先写 `UserInputSubmitted`，其 trace 固定 `entry_index=0`；随后写 `TurnStarted`，其 trace `entry_index=None`，见 `crates/agentdash-application/src/session/launch/commit.rs:117`、`crates/agentdash-application/src/session/launch/commit.rs:125`、`crates/agentdash-application/src/session/launch/commit.rs:141`。构造函数分别见 `crates/agentdash-application/src/session/hub_support.rs:14` 和 `crates/agentdash-application/src/session/hub_support.rs:39`。

当前主线 terminal 并不是一定写 `BackboneEvent::TurnCompleted`。`SessionTurnProcessor` 收到 `TurnEvent::Terminal` 后构造并持久化 `turn_terminal` platform event：`BackboneEvent::Platform(SessionMetaUpdate { key: "turn_terminal", value: { terminal_type, message } })`，见 `crates/agentdash-application/src/session/turn_processor.rs:124`、`crates/agentdash-application/src/session/turn_processor.rs:132`、`crates/agentdash-application/src/session/hub_support.rs:78`、`crates/agentdash-application/src/session/hub_support.rs:85`。`terminal_type` 可为 `turn_completed` / `turn_failed` / `turn_interrupted` / `turn_lost`，解析见 `crates/agentdash-application/src/session/hub_support.rs:103`。

Terminal 处理顺序是稳定边界的核心：先持久化 terminal event，然后无论是否落库成功都清理 active turn；只有 terminal event 成功后才 broadcast 并 dispatch terminal effects，见 `crates/agentdash-application/src/session/turn_processor.rs:132`、`crates/agentdash-application/src/session/turn_processor.rs:137`、`crates/agentdash-application/src/session/turn_processor.rs:143`、`crates/agentdash-application/src/session/turn_processor.rs:162`。测试 `terminal_persist_failure_still_clears_active_turn` 固化了“terminal persist 失败也释放 active turn”，见 `crates/agentdash-application/src/session/turn_processor.rs:232`。

Repository projection 对 terminal 的认知来自 `projection_from_envelope`：`TurnStarted` -> `last_delivery_status=running` 且清空 terminal message；`TurnCompleted` 根据 `Turn.status` 映射 completed/failed/interrupted；`Error` 直接映射 failed；`turn_terminal` platform payload 根据 `terminal_type` 映射 completed/failed/interrupted/lost，见 `crates/agentdash-infrastructure/src/persistence/session_core.rs:671`、`crates/agentdash-infrastructure/src/persistence/session_core.rs:686`、`crates/agentdash-infrastructure/src/persistence/session_core.rs:691`、`crates/agentdash-infrastructure/src/persistence/session_core.rs:705`、`crates/agentdash-infrastructure/src/persistence/session_core.rs:714`。

`SessionExecutionState` 优先读内存 runtime registry；无 running/cancelling 时才从 `SessionMeta` 投影，见 `crates/agentdash-application/src/session/core.rs:150`。`meta_to_execution_state` 将 persisted `ExecutionStatus::Failed` 转成 `SessionExecutionState::Failed { turn_id, message }`，缺少 failed/completed `last_turn_id` 会被视为 invalid data，见 `crates/agentdash-application/src/session/hub_support.rs:352`。

结论：当前稳定完成轮次的最可靠后端边界是 `turn_terminal.terminal_type == "turn_completed"` 或兼容的 `BackboneEvent::TurnCompleted(status=completed)`，而不是 `TurnStarted`。失败/中断/丢失轮次的 terminal boundary 是 `turn_terminal` 的 failed/interrupted/lost，它标记“本轮已终止”，但不代表该轮 producer events 对下一次 model context 稳定可见。

### event_seq / entry_index / session_update_type Semantics

`event_seq` 是唯一全局 session 事件顺序，也是前端 backlog、NDJSON resume、repository restore、projection suffix 的顺序来源。前端 reducer 会按 `event_seq` 排序，并跳过 `event_seq <= lastAppliedSeq`，见 `packages/app-web/src/features/session/model/sessionStreamReducer.ts:279`、`packages/app-web/src/features/session/model/sessionStreamReducer.ts:287`、`packages/app-web/src/features/session/model/sessionStreamReducer.ts:294`。

`entry_index` 不是稳定完成边界，而是同一 turn 内消息/工具/delta 的归并坐标。Pi Agent stream mapper 按 entry index 合并 assistant message delta、reasoning、工具调用；frontend reducer 用 `turn_id + entry_index` 生成 delta entry id，见 `packages/app-web/src/features/session/model/sessionStreamReducer.ts:57` 和 `packages/app-web/src/features/session/model/sessionStreamReducer.ts:66`。Repository restore 也用 `turn_id + entry_index` 作为 assistant key，见 `crates/agentdash-application/src/session/continuation.rs:430`。

`session_update_type` 是 `backbone_event_type_name(&envelope.event)` 的归档标签，写入 `PersistedSessionEvent.session_update_type`，见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:353`。它适合做粗略分类和前端/调试显示，但 stable boundary 需要读 `notification.event` 的具体类型/payload，因为主线 terminal 同样是 `session_update_type="platform_event"`，真正 terminal kind 在 `notification.event.payload.data.key == "turn_terminal"` 与 `value.terminal_type` 中。

### How Persisted Events Become Projection / Resume Input

`SessionEventingService.persist_notification_inner` 是普通事件进入 store/projection/broadcast 的主入口。它先对 compaction completed 事件走 projection commit 特例；否则 append event，然后 `advance_model_projection_head`，再 broadcast，见 `crates/agentdash-application/src/session/eventing.rs:150`、`crates/agentdash-application/src/session/eventing.rs:157`、`crates/agentdash-application/src/session/eventing.rs:182`、`crates/agentdash-application/src/session/eventing.rs:187`、`crates/agentdash-application/src/session/eventing.rs:189`。

模型上下文读取由 `ContextProjector` 负责，而不是前端 timeline。`build_model_context` 先读全部 events，再读 `session_projection_heads(model_context)`；有 head 时从 projection head/segments 加 suffix events，没 head 时从全部 raw events 重建 transcript，见 `crates/agentdash-application/src/session/context_projector.rs:29`、`crates/agentdash-application/src/session/context_projector.rs:37`、`crates/agentdash-application/src/session/context_projector.rs:42`。有 compaction head 时，读取 active compaction 和 projection segments，再从 `suffix_start_event_seq` 到 `head.head_event_seq` 的事件构造 suffix，见 `crates/agentdash-application/src/session/context_projector.rs:177`、`crates/agentdash-application/src/session/context_projector.rs:190`、`crates/agentdash-application/src/session/context_projector.rs:200`、`crates/agentdash-application/src/session/context_projector.rs:202`、`crates/agentdash-application/src/session/context_projector.rs:205`。

Raw transcript restore 当前只看实际持久化事件：

- `UserInputSubmitted` 进入 user message，key 是 `input.item_id`，见 `crates/agentdash-application/src/session/continuation.rs:251`。
- `AgentMessageDelta` / reasoning deltas 进入 assistant message，key 优先 `turn_id + entry_index`，见 `crates/agentdash-application/src/session/continuation.rs:270`、`crates/agentdash-application/src/session/continuation.rs:286`、`crates/agentdash-application/src/session/continuation.rs:304`。
- Tool item started/completed 会补 assistant tool calls 和 terminal tool results，见 `crates/agentdash-application/src/session/continuation.rs:322` 和 `crates/agentdash-application/src/session/continuation.rs:350`。
- 最终按 event order 排序为 `ProjectedTranscript`，见 `crates/agentdash-application/src/session/continuation.rs:420`。

下一次 AgentRun 输入上下文重建主线如下：

1. 前端 composer 走 AgentRun mailbox command，不直接调用 session trace command。`AgentRunMailboxService::accept_user_message_for_target` 先读取 current `SessionExecutionState`、claim command receipt、创建 durable mailbox message，再 schedule，见 `crates/agentdash-application/src/agent_run/mailbox.rs:247`、`crates/agentdash-application/src/agent_run/mailbox.rs:291`、`crates/agentdash-application/src/agent_run/mailbox.rs:326`、`crates/agentdash-application/src/agent_run/mailbox.rs:354`、`crates/agentdash-application/src/agent_run/mailbox.rs:391`。
2. Scheduler claim 后，`consume_as_launch` 通过 `SessionTurnMessageDeliveryPort` 调用 `SessionLaunchService::launch_command_in_task`，见 `crates/agentdash-application/src/agent_run/mailbox.rs:1404`、`crates/agentdash-application/src/agent_run/mailbox.rs:1424`、`crates/agentdash-application/src/agent_run/message_delivery.rs:37`、`crates/agentdash-application/src/agent_run/message_delivery.rs:54`。
3. Launch planner 根据 `RuntimeTraceLaunchState::from(SessionMeta)`、`has_live_executor_session`、`supports_repository_restore` 判定 launch path。冷启动、无 live runtime、有历史事件、无 `executor_session_id` 时进入 `RepositoryRehydrate`；如果 connector 支持 repository restore，则 mode 是 `ExecutorState`，见 `crates/agentdash-application/src/session/types.rs:150`、`crates/agentdash-application/src/session/types.rs:168`、`crates/agentdash-application/src/session/types.rs:183`。
4. `LaunchPlanner` 在 `RepositoryRehydrate(ExecutorState)` 下调用 `eventing.build_projected_transcript(session_id)`，把 entries 转成 `RestoredSessionState { messages, message_refs }`，见 `crates/agentdash-application/src/session/launch/planner.rs:177`、`crates/agentdash-application/src/session/launch/planner.rs:181`、`crates/agentdash-application/src/session/launch/planner.rs:192`。
5. `LaunchPlan::build` 把 `restored_session_state` 放入 `ExecutionTurnFrame`，见 `crates/agentdash-application/src/session/launch/plan.rs:130`、`crates/agentdash-application/src/session/launch/plan.rs:281`。
6. Pi Agent connector 在新建或因模型变化重建 agent 时，如果 `restored_session_state` 非空，就 `replace_messages_with_refs`，然后再投递当前新 user prompt，见 `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:652`、`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:770`。

关键影响：当前 repository restore 会从 projection/events 直接重建 transcript；如果最后 failed turn 的 user/assistant/tool events 仍然在 active projection head 范围内，下一次请求会把它们当作事实恢复，除非新增稳定边界过滤或 rollback marker 语义。

### How Persisted Events Become Frontend Backlog / Feed

后端 `/sessions/{id}/events` 调用 `SessionEventingService::list_event_page` 返回 `SessionEventsPageResponse { snapshot_seq, events, has_more, next_after_seq }`，见 `crates/agentdash-api/src/routes/sessions.rs:418`。wire DTO 保留 `event_seq`、`session_update_type`、`turn_id`、`entry_index`、`tool_call_id`、`notification`，见 `crates/agentdash-contracts/src/runtime/session.rs:14`。

后端 `/sessions/{id}/stream/ndjson` 从 `x-stream-since-id` / query `since_id` 恢复，调用 `subscribe_after` 先补发 backlog，再发 connected，随后只发送 `event_seq > subscription.snapshot_seq` 的新事件，见 `crates/agentdash-api/src/routes/sessions.rs:977`、`crates/agentdash-api/src/routes/sessions.rs:985`、`crates/agentdash-api/src/routes/sessions.rs:994`、`crates/agentdash-api/src/routes/sessions.rs:1008`、`crates/agentdash-api/src/routes/sessions.rs:1027`。`SessionEventingService::subscribe_after` 读取 backlog 的实现见 `crates/agentdash-application/src/session/eventing.rs:66`。

前端 `useSessionStream` 启动时先分页 `fetchSessionEvents(sessionId, afterSeq, 500)` hydrate 历史，再以 `sinceId = nextState.lastAppliedSeq` 建立 NDJSON stream，见 `packages/app-web/src/features/session/model/useSessionStream.ts:196`、`packages/app-web/src/features/session/model/useSessionStream.ts:202`、`packages/app-web/src/features/session/model/useSessionStream.ts:215`。`streamTransport` 对每条 event 要求合法 `event_seq`、`session_id`、`occurred_at_ms`、`committed_at_ms`、`session_update_type`、`notification`，见 `packages/app-web/src/features/session/model/streamTransport.ts:84`。

前端 reducer 的事实源是 `rawEvents`；`entries` 是派生显示状态。它按 `event_seq` 去重，并用 event payload / `turn_id` / `entry_index` 聚合 display entries，见 `packages/app-web/src/features/session/model/useSessionStream.ts:4`、`packages/app-web/src/features/session/model/sessionStreamReducer.ts:11`、`packages/app-web/src/features/session/model/sessionStreamReducer.ts:279`。

当前 `sessionStreamReducer` 明确忽略 `turn_started` / `turn_completed` display entry，见 `packages/app-web/src/features/session/model/sessionStreamReducer.ts:215`。`useSessionFeed.segmentByTurn` 只从 rawEvents 中的 `BackboneEvent::TurnCompleted` 提取 turn status/duration，见 `packages/app-web/src/features/session/model/useSessionFeed.ts:360`、`packages/app-web/src/features/session/model/useSessionFeed.ts:365`。由于后端主线 terminal 是 `platform/session_meta_update key=turn_terminal`，frontend turn segmentation 当前不会从主线 terminal event 得到 failed/completed meta。这个差异会影响 failed turn 的可见状态和“丢弃失败轮次”的前端刷新策略。

### AgentRun Mailbox / Workspace Boundary

AgentRun workspace command 的 durable fact source 是 mailbox envelope，不是前端键盘状态，也不是 RuntimeSession trace 直接分支。`user_message_policy` 在 running/cancelling 时把用户输入设为 `AgentRunTurnBoundary` queued message；idle/completed/failed/interrupted/lost 时设为 `ImmediateIfIdle` launch message，见 `crates/agentdash-application/src/agent_run/mailbox.rs:2081`。`runtime_can_launch` 明确允许 `Failed` / `Interrupted` / `Lost` 后再 launch，见 `crates/agentdash-application/src/agent_run/mailbox.rs:2121`。

Scheduler 每次先 `recover_expired_consuming`，再读 `SessionExecutionState`，按 trigger 选择 barrier/drain mode 进行 `claim_and_consume`，见 `crates/agentdash-application/src/agent_run/mailbox.rs:1007`、`crates/agentdash-application/src/agent_run/mailbox.rs:1024`、`crates/agentdash-application/src/agent_run/mailbox.rs:1026`、`crates/agentdash-application/src/agent_run/mailbox.rs:1039`。被 claim 的消息消费成功后才写 `Dispatched` 或 `Steered`、accepted refs 和 command receipt，见 `crates/agentdash-application/src/agent_run/mailbox.rs:1171`、`crates/agentdash-application/src/agent_run/mailbox.rs:1479`、`crates/agentdash-application/src/agent_run/mailbox.rs:1490`、`crates/agentdash-application/src/agent_run/mailbox.rs:1580`、`crates/agentdash-application/src/agent_run/mailbox.rs:1600`。

Terminal callback 会改变 mailbox 行为：completed terminal 调度 `AgentRunTurnBoundary`，failed/interrupted terminal 则 `pause_for_terminal`，见 `crates/agentdash-api/src/agent_run_mailbox.rs:78`、`crates/agentdash-api/src/agent_run_mailbox.rs:81`、`crates/agentdash-api/src/agent_run_mailbox.rs:91`、`crates/agentdash-api/src/agent_run_mailbox.rs:108`。因此“丢弃最后失败轮次”若希望自动/手动 retry 后继续，需要明确处理 mailbox pause state，否则 runtime 可 launch 但 mailbox projection 仍暂停。

AgentRun workspace projection 从 `SessionExecutionState` 派生 `workspace_status`/delivery status：failed state 保留 last_turn_id 和 message，见 `crates/agentdash-application/src/agent_run/workspace/projection.rs:37`、`crates/agentdash-application/src/agent_run/workspace/projection.rs:78`、`crates/agentdash-application/src/agent_run/workspace/projection.rs:93`。这说明 failed terminal 对 UI 是诊断事实，不等同于可进入下一次 model context 的稳定事实。

### Candidate Boundaries For Dropping The Last Failed Turn

#### Candidate A: Projection-only stable boundary filter

新增模型上下文 read model 过滤规则：在 `ContextProjector` / `continuation` 构建 transcript 前，识别最后一个 terminal `turn_terminal`。若最后 terminal 是 failed/lost/interrupted，则从 projected transcript 中排除该 `turn_id` 的 provider-produced events；稳定 head 回到上一 `turn_terminal.turn_completed` 或该 failed turn `TurnStarted` 之前的 event。实现点主要是 `ContextProjector::build_model_context` 和 `build_raw_projected_transcript_from_filtered_events` 前的 event filter，见 `crates/agentdash-application/src/session/context_projector.rs:29` 和 `crates/agentdash-application/src/session/continuation.rs:230`。

优点：

- 不破坏 `event_seq` 单调性和 NDJSON resume。
- 不需要物理删除历史事件，失败诊断仍可展示和审计。
- 符合现有 compaction projection “真实事件不改写，projection 派生模型输入” 的方向。

风险：

- 前端 `rawEvents` 仍含半截失败 turn；如果不新增 rollback/stable marker 或刷新机制，用户仍会看到被模型上下文排除的失败输出。
- 需要定义 failed turn 中的 `UserInputSubmitted` 是否排除。若目标是“下一次 retry 重新提交同一用户输入”，通常应从 model context 排除该 turn 的 user input 和 provider events；但 mailbox/command receipt 仍保留用户曾提交的事实。
- 如果 failed turn 期间产生 context_frame、capability/runtime command accepted frames，过滤时要决定是否也排除这些 turn-scoped frames。当前 context frames 也按 event stream 恢复和展示，见 `crates/agentdash-application/src/session/eventing.rs:281`。

#### Candidate B: Append rollback / stable-boundary marker

在 failed/lost/interrupted terminal 后追加一个结构化 platform event，例如 `SessionMetaUpdate key="turn_projection_rolled_back"` 或一等 `PlatformEvent`，payload 包含 `failed_turn_id`、`stable_event_seq`、`stable_turn_id`、`reason`。`ContextProjector` 和前端 reducer/feed 读 marker 进行过滤/刷新。

优点：

- 保持 append-only event log，不破坏 `event_seq`。
- 前端有显式信号可修剪或刷新，而不是自己猜最后 failed turn。
- 可以同时表达诊断保留和 model context rollback。

风险：

- 需要新增协议/contract/生成 TS 和前端处理。
- 要保证 marker 的 `stable_event_seq` 与 compaction projection head 语义一致。若 active compaction head 已覆盖 failed turn 前缀，rollback marker 要么只影响 suffix，要么需要 projection head rollback 能力。

#### Candidate C: Explicit `last_stable_event_seq` / `last_stable_turn_id` persisted projection

在 SessionMeta 或专门 read model 中维护最后稳定完成边界。`TurnStarted` 不更新 stable；`turn_terminal.turn_completed` 更新 stable；failed/lost/interrupted 不更新 stable，并让 repository restore 使用 stable head。

优点：

- 下一次 restore 读取很直接，不必每次扫描全部 events 找最后 completed terminal。
- 可以给 workspace 和 debug API 明确暴露 stable head。

风险：

- 需要 migration 和回填策略。
- 仅有 stable head 不足以处理 compaction head；`ContextProjector` 仍需知道在 stable head 之前如何组合 projection head/segments。
- 如果只改 meta，不改 events/feed，前端 display 仍会显示失败半截。

#### Candidate D: Physical tail deletion / event truncation

新增 repository 方法删除 `session_events WHERE event_seq > stable_event_seq`，并把 `sessions.last_event_seq`、`last_delivery_status`、`last_turn_id` 等重算到 stable event。

优点：

- 前端 backlog 和 repository restore 天然一致，失败半截不再出现。

风险：

- 当前 `SessionEventStore` 没有 truncate API，见 `crates/agentdash-spi/src/session_persistence.rs:797`。
- `event_seq` 被 NDJSON `x-stream-since-id` 和前端 `lastAppliedSeq` 用作恢复游标。删除尾部后已连接客户端可能持有大于新 head 的 cursor，导致补发/connected 语义变复杂。
- `save_session_meta` 的 `GREATEST(last_event_seq)` 保护意味着普通 meta save 无法回退 head，见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:229`。
- terminal effect outbox、mailbox accepted refs、command receipt 可能已经引用被删 terminal event seq / turn id。需要跨 store 清理，blast radius 大。

### Recommended Boundary For This Task

建议优先采用“append-only rollback marker + projection filter”的组合：

- **Stable turn boundary**：`turn_terminal.terminal_type == "turn_completed"`，兼容 `BackboneEvent::TurnCompleted(turn.status == completed)`。
- **Unstable failed turn boundary**：最后一个 `turn_terminal.terminal_type in ["turn_failed", "turn_lost", "turn_interrupted"]` 对应的 `turn_id`。
- **Model context boundary**：下一次 repository restore / `ContextProjector` 使用 `stable_event_seq` 作为 head，或在 raw event restore 时排除 unstable `turn_id` 的 user/assistant/tool/context events。
- **UI/feed boundary**：前端消费 rollback marker 或 workspace refresh，修剪/隐藏 `event_seq > stable_event_seq` 且 `turn_id == failed_turn_id` 的 display entries，同时保留一个系统诊断事件。
- **Mailbox boundary**：failed terminal 已 pause mailbox；retry/reconnect 设计需要明确是自动 resume、保留 paused 等待用户、还是在 rollback marker 后清除特定 pause。相关入口为 `AgentRunMailboxTerminalCallback::on_session_terminal` 和 `AgentRunMailboxService::pause_for_terminal`，见 `crates/agentdash-api/src/agent_run_mailbox.rs:80` 和 `crates/agentdash-application/src/agent_run/mailbox.rs:956`。

不建议第一版物理删除尾部事件，除非任务明确要求 event log 也不可见失败半截。当前 store、stream cursor、meta merge、terminal effect/mailbox 引用都更适合 append-only projection 修正。

## External References

- No external web references used in this research turn.
- Internal task references read:
  - `.trellis/tasks/06-23-agent-provider-retry-reconnect/prd.md`
  - `.trellis/tasks/06-23-agent-provider-retry-reconnect/design.md`
  - `.trellis/tasks/06-23-agent-provider-retry-reconnect/implement.md`

## Related Specs

- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/context-compaction-projection.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/backend/session/streaming-protocol.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/frontend/index.md`
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`

## Caveats / Not Found

- Active task lookup returned none from `python ./.trellis/scripts/task.py current --source`; this research used the explicit task path provided in the prompt: `.trellis/tasks/06-23-agent-provider-retry-reconnect`.
- No `SessionEventStore` tail truncate / rollback API was found. Current store methods are append/read/list only.
- No existing durable `last_stable_event_seq` / `last_stable_turn_id` field was found in `SessionMeta`.
- Frontend `segmentByTurn` currently reads `BackboneEvent::TurnCompleted`, while backend mainline terminal is `Platform(SessionMetaUpdate key="turn_terminal")`; any failed-turn UI recovery relying only on existing frontend segmentation will miss current terminal facts.
- This research focused on session/event/projection/restore/mailbox/feed boundaries. It did not re-audit every provider bridge or retry classifier path; those are covered by other task research artifacts.
