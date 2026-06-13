# Research: runtime-hook-mailbox-integration

- Query: AgentRun mailbox scheduler 需要接入的 hook/runtime 边界、现有调用链、锚点和迁移写集
- Scope: internal
- Date: 2026-06-13

## Findings

### Files Found

- `crates/agentdash-agent/src/types.rs` - 定义 `AgentEvent::TurnStart` / `AgentEvent::TurnEnd`。
- `crates/agentdash-agent/src/agent_loop.rs` - Agent loop 主循环、`TurnEnd` 后 runtime delegate、`poll_steering` / `poll_follow_up`、`pending_follow_up_messages` 归并逻辑。
- `crates/agentdash-agent/src/agent.rs` - `QueueMode::All`、`steering_queue`、`follow_up_queue`、`Agent::steer`、`dequeue_messages_inner`。
- `crates/agentdash-agent-types/src/runtime/delegate.rs` - `AgentRuntimeDelegate` 的 `after_turn` / `before_stop` trait 边界。
- `crates/agentdash-agent-types/src/runtime/decisions.rs` - `TurnControlDecision`、`StopDecision`、`TransformContextOutput`。
- `crates/agentdash-application/src/session/hook_delegate.rs` - `HookRuntimeDelegate` 实现 UserPromptSubmit / AfterTurn / BeforeStop hook runtime。
- `crates/agentdash-spi/src/hooks/mod.rs` - `HookRuntimeAccess`、`HookControlTarget`、`RuntimeAdapterProvenance`、`HookPendingAction`。
- `crates/agentdash-application/src/session/launch/planner.rs` - launch 时创建 `HookRuntimeDelegate` 并放入 `LaunchPlanInput.runtime_delegate`。
- `crates/agentdash-application/src/session/launch/plan.rs` - `LaunchPlan.runtime_delegate` 传入 `ExecutionTurnFrame`。
- `crates/agentdash-spi/src/connector/mod.rs` - `ExecutionTurnFrame.runtime_delegate`、`supports_session_steering`、`steer_session` connector 边界。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - composer submit、pending queue promote/resume/delete、`classify_composer_submit_kind`。
- `crates/agentdash-application/src/workflow/agent_message.rs` - idle / next-turn delivery 的 `AgentRunMessageService` 和 command receipt。
- `crates/agentdash-application/src/workflow/agent_steering.rs` - active-turn steering delivery 的 `AgentRunSteeringService`。
- `crates/agentdash-application/src/session/control.rs` - `SessionControlService::steer_session`。
- `crates/agentdash-application/src/relay_connector.rs` - relay connector steering 转发。
- `crates/agentdash-domain/src/workflow/mailbox.rs` - mailbox domain model、claim、status、repository trait。
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql` - mailbox tables、状态、索引和 command receipt `mailbox_message_id`。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs` - mailbox Postgres repository。
- `crates/agentdash-contracts/src/workflow.rs` - `MailboxMessageView` / `MailboxStateView` 已存在，但 workspace/runtime control 仍暴露 pending queue。
- `crates/agentdash-application/src/session/pending_queue.rs` - 当前 in-memory pending queue。
- `crates/agentdash-api/src/agent_run_pending.rs` - pending terminal callback、completed drain、failed/interrupted pause、resume。
- `crates/agentdash-api/src/bootstrap/session.rs` - 注入 `AgentRunPendingTerminalCallback` 到 composite terminal callback。
- `crates/agentdash-application/src/session/turn_processor.rs` - terminal event 处理顺序、清 active turn、terminal effects dispatch。
- `crates/agentdash-application/src/session/terminal_effects.rs` - `TerminalEffectType::HookAutoResume` outbox enqueue/replay/execute。
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs` - `request_hook_auto_resume` / `schedule_hook_auto_resume` direct launch。
- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs` - `RuntimeSessionExecutionAnchor` 的 run / agent / frame / runtime session 锚点。
- `crates/agentdash-domain/src/workflow/repository.rs` - `RuntimeSessionExecutionAnchorRepository::find_by_session`。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` - runtime session anchor Postgres 查询。
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` - 前端仍使用 pending messages、pending promote/resume/delete、composer submit old response。
- `packages/app-web/src/services/lifecycle.ts` - composer submit service。
- `packages/app-web/src/services/lifecycle.test.ts` - pending endpoint tests。

### Related Specs

- `.trellis/spec/backend/session/agentrun-mailbox.md` - AgentRun mailbox 的 command envelope、delivery mode、barrier、status、pause/resume、投影要求。
- `.trellis/spec/backend/session/runtime-execution-state.md` - runtime session execution state 与 active turn / terminal state 推导。
- `.trellis/spec/backend/session/execution-context-frames.md` - run / agent / frame 锚点语义。
- `.trellis/spec/backend/session/architecture.md` - session lifecycle、runtime session、anchor 和 terminal callback 约束。
- `.trellis/spec/backend/hooks/execution-hook-runtime.md` - hook runtime trigger、pending action、runtime injection、权限与锚点。
- `.trellis/spec/backend/hooks/architecture.md` - hook 执行模型和 trigger 归属。
- `.trellis/spec/backend/runtime-gateway.md` - runtime gateway / connector 边界。

### Agent Loop Boundary and Queue Semantics

- `AgentEvent::TurnEnd` 定义在 `crates/agentdash-agent/src/types.rs:35` 的 `AgentEvent` enum 内，具体 variant 在 `crates/agentdash-agent/src/types.rs:42`。`TurnStart` 在 `crates/agentdash-agent/src/types.rs:41`。
- Agent loop 每轮初始和后续 turn start 分别在 `crates/agentdash-agent/src/agent_loop.rs:133`、`crates/agentdash-agent/src/agent_loop.rs:189`、`crates/agentdash-agent/src/agent_loop.rs:233` 发出。
- `AgentEvent::TurnEnd` 在错误/abort 路径从 `crates/agentdash-agent/src/agent_loop.rs:266` 发出，正常路径从 `crates/agentdash-agent/src/agent_loop.rs:311` 发出。
- 正常 `TurnEnd` 后，主循环先调用 `run_after_turn_delegate(...)`，位置是 `crates/agentdash-agent/src/agent_loop.rs:318`；helper 定义在 `crates/agentdash-agent/src/agent_loop.rs:381`。
- `decision.steering` 被追加到本轮后续 `pending_messages`，位置是 `crates/agentdash-agent/src/agent_loop.rs:322` 到 `crates/agentdash-agent/src/agent_loop.rs:323`。
- `decision.follow_up` 被追加到 `pending_follow_up_messages`，位置是 `crates/agentdash-agent/src/agent_loop.rs:325` 到 `crates/agentdash-agent/src/agent_loop.rs:326`。
- `poll_steering(config)` 在 `crates/agentdash-agent/src/agent_loop.rs:330` 再次轮询 in-memory steering queue，结果追加到 `pending_messages`，位置是 `crates/agentdash-agent/src/agent_loop.rs:331`。
- loop 内部结束后，`pending_follow_up_messages` 会在 `crates/agentdash-agent/src/agent_loop.rs:334` 到 `crates/agentdash-agent/src/agent_loop.rs:336` 归并为下一轮 `pending_messages`。
- `poll_follow_up(config)` 在 `crates/agentdash-agent/src/agent_loop.rs:339` 读取 follow-up queue。
- `run_before_stop_delegate(...)` 在 `crates/agentdash-agent/src/agent_loop.rs:345` 调用；`StopDecision::Continue { steering, follow_up, ... }` 的消息分别在 `crates/agentdash-agent/src/agent_loop.rs:348` 到 `crates/agentdash-agent/src/agent_loop.rs:359` 追加后继续 loop。
- `poll_steering` 定义在 `crates/agentdash-agent/src/agent_loop.rs:427`，只是同步调用 `config.get_steering_messages`；`poll_follow_up` 定义在 `crates/agentdash-agent/src/agent_loop.rs:435`。
- `QueueMode` 定义在 `crates/agentdash-agent/src/agent.rs:36` 到 `crates/agentdash-agent/src/agent.rs:43`，包含 `All` 和 `OneAtATime`。
- `AgentConfig.steering_mode` 和 `AgentConfig.follow_up_mode` 分别在 `crates/agentdash-agent/src/agent.rs:59` 到 `crates/agentdash-agent/src/agent.rs:63`。
- `steering_queue` 和 `follow_up_queue` 字段在 `crates/agentdash-agent/src/agent.rs:96` 到 `crates/agentdash-agent/src/agent.rs:97`，初始化在 `crates/agentdash-agent/src/agent.rs:122` 到 `crates/agentdash-agent/src/agent.rs:123`。
- `Agent::steer` 在 `crates/agentdash-agent/src/agent.rs:242` 到 `crates/agentdash-agent/src/agent.rs:244` push 到 `steering_queue`。
- `Agent::follow_up` 在 `crates/agentdash-agent/src/agent.rs:248` 到 `crates/agentdash-agent/src/agent.rs:249` push 到 `follow_up_queue`。
- `Agent::continue_loop` 会在 last message 是 assistant 时优先读取 `steering_queue`，位置是 `crates/agentdash-agent/src/agent.rs:383` 到 `crates/agentdash-agent/src/agent.rs:397`。
- `Agent::run_loop` 把 queue 和 mode clone 进 `AgentLoopConfig`，位置是 `crates/agentdash-agent/src/agent.rs:429` 到 `crates/agentdash-agent/src/agent.rs:434`；`get_steering_messages` closure 在 `crates/agentdash-agent/src/agent.rs:488` 到 `crates/agentdash-agent/src/agent.rs:492`；`get_follow_up_messages` 在 `crates/agentdash-agent/src/agent.rs:494` 到 `crates/agentdash-agent/src/agent.rs:495`。
- `dequeue_messages_inner` 定义在 `crates/agentdash-agent/src/agent.rs:648` 到 `crates/agentdash-agent/src/agent.rs:655`，其中 `QueueMode::All => std::mem::take(queue)`。
- 结论：当前 `QueueMode::All` 是 connector/agent 内存队列一次性 drain，不是 durable mailbox drain。`poll_steering` 和 `GetMessagesFn` 是同步函数，不能直接 await database-backed scheduler。Mailbox scheduler 若要在 AgentLoopTurn boundary 接入，需要新增 async boundary callback，或通过 `AgentRuntimeDelegate` composite/扩展在 `TurnEnd` 后、`poll_steering` 前把 claimed mailbox 消息作为 `TurnControlDecision.steering` 返回。

### Application / API Trigger Points

- composer submit 入口是 `submit_agent_run_composer_input`，定义在 `crates/agentdash-api/src/routes/lifecycle_agents.rs:171`。
- route 读取 execution state 并检查 steering support，位置是 `crates/agentdash-api/src/routes/lifecycle_agents.rs:217` 到 `crates/agentdash-api/src/routes/lifecycle_agents.rs:226`。
- `classify_composer_submit_kind(...)` 调用在 `crates/agentdash-api/src/routes/lifecycle_agents.rs:227` 到 `crates/agentdash-api/src/routes/lifecycle_agents.rs:231`；函数定义在 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1677` 到 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1697`。
- `SendNext` 分支在 `crates/agentdash-api/src/routes/lifecycle_agents.rs:248` 调用 `dispatch_message_for_runtime`。
- `Enqueue` 分支在 `crates/agentdash-api/src/routes/lifecycle_agents.rs:268`，通过 `state.services.pending_queue.enqueue(...)` 写 in-memory pending queue，位置是 `crates/agentdash-api/src/routes/lifecycle_agents.rs:274` 到 `crates/agentdash-api/src/routes/lifecycle_agents.rs:278`，并用 `accepted_receipt(req.client_command_id)` 返回 synthetic receipt，位置是 `crates/agentdash-api/src/routes/lifecycle_agents.rs:282`。
- `Steer` 分支在 `crates/agentdash-api/src/routes/lifecycle_agents.rs:295` 调用 `steer_runtime_session`，返回 synthetic accepted receipt 的 helper 在 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1209` 到 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1216`。
- pending promote 入口 `promote_pending_message_for_runtime` 定义在 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1256`，读取 pending queue 的位置是 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1261` 到 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1265`。
- next-turn launch delivery 在 `AgentRunMessageService::dispatch_user_message`，定义于 `crates/agentdash-application/src/workflow/agent_message.rs:123` 到 `crates/agentdash-application/src/workflow/agent_message.rs:126`。
- `AgentRunMessageService` 使用 `RuntimeSessionExecutionAnchorRepository::find_by_session` 解析 run / agent / frame anchor，位置是 `crates/agentdash-application/src/workflow/agent_message.rs:144` 到 `crates/agentdash-application/src/workflow/agent_message.rs:145` 以及 `crates/agentdash-application/src/workflow/agent_message.rs:216` 到 `crates/agentdash-application/src/workflow/agent_message.rs:222`。
- `AgentRunMessageService` 当前通过 `claim_agent_run_command_receipt` 做 command idempotency，位置是 `crates/agentdash-application/src/workflow/agent_message.rs:154` 到 `crates/agentdash-application/src/workflow/agent_message.rs:168`。
- active-turn steering delivery 在 `AgentRunSteeringService::steer`，定义于 `crates/agentdash-application/src/workflow/agent_steering.rs:61`。
- `AgentRunSteeringService` 用 `RuntimeSessionExecutionAnchorRepository::find_by_session` 解析 anchor，位置是 `crates/agentdash-application/src/workflow/agent_steering.rs:76` 到 `crates/agentdash-application/src/workflow/agent_steering.rs:84`。
- `AgentRunSteeringService` 从 `SessionCoreService::inspect_session_execution_state` 读取 active turn，位置是 `crates/agentdash-application/src/workflow/agent_steering.rs:135` 到 `crates/agentdash-application/src/workflow/agent_steering.rs:144`。
- `AgentRunSteeringService` 校验 `supports_session_steering` 后调用 `SessionControlService::steer_session`，位置是 `crates/agentdash-application/src/workflow/agent_steering.rs:155` 到 `crates/agentdash-application/src/workflow/agent_steering.rs:171`。
- `SessionControlService::steer_session` 定义在 `crates/agentdash-application/src/session/control.rs:33` 到 `crates/agentdash-application/src/session/control.rs:43`，connector trait 边界在 `crates/agentdash-spi/src/connector/mod.rs:767` 到 `crates/agentdash-spi/src/connector/mod.rs:776`。
- relay connector 的 `steer_session` 转发在 `crates/agentdash-application/src/relay_connector.rs:277` 到 `crates/agentdash-application/src/relay_connector.rs:303`。
- 结论：从 AgentDash application/API 层触发 mailbox scheduler 的现有替换点是 composer submit route、pending promote/resume/delete route、terminal callback、hook auto-resume effect executor，以及 AgentLoopTurn runtime delegate boundary。现有 launch 和 steer 传输可以复用，但应被 scheduler claim/mark 包裹，而不是由 route 直接选择 `SendNext` / `Enqueue` / `Steer`。

### Hook Decisions: Anchored Delivery vs Runtime Strategy

- `AgentRuntimeDelegate` trait 的 `after_turn(...) -> TurnControlDecision` 定义在 `crates/agentdash-agent-types/src/runtime/delegate.rs:64` 到 `crates/agentdash-agent-types/src/runtime/delegate.rs:68`。
- `AgentRuntimeDelegate` trait 的 `before_stop(...) -> StopDecision` 定义在 `crates/agentdash-agent-types/src/runtime/delegate.rs:70` 到 `crates/agentdash-agent-types/src/runtime/delegate.rs:74`。
- `TurnControlDecision` 定义在 `crates/agentdash-agent-types/src/runtime/decisions.rs:103` 到 `crates/agentdash-agent-types/src/runtime/decisions.rs:107`，包含 `steering`、`follow_up`、`refresh_snapshot`、`diagnostics`。
- `StopDecision` 定义在 `crates/agentdash-agent-types/src/runtime/decisions.rs:116` 到 `crates/agentdash-agent-types/src/runtime/decisions.rs:123`，`Continue` variant 包含 `steering`、`follow_up`、`reason`、`allow_empty`。
- `TransformContextOutput.steering_messages` 定义在 `crates/agentdash-agent-types/src/runtime/decisions.rs:27` 到 `crates/agentdash-agent-types/src/runtime/decisions.rs:33`，是 UserPromptSubmit transform/context injection 路径的一部分。
- `HookRuntimeDelegate::transform_context` 对 `HookTrigger::UserPromptSubmit` 求值，位置是 `crates/agentdash-application/src/session/hook_delegate.rs:531` 到 `crates/agentdash-application/src/session/hook_delegate.rs:542`。
- UserPromptSubmit block result 在 `crates/agentdash-application/src/session/hook_delegate.rs:544` 到 `crates/agentdash-application/src/session/hook_delegate.rs:557` 返回 `blocked`。
- UserPromptSubmit context injection trace 在 `crates/agentdash-application/src/session/hook_delegate.rs:570` 到 `crates/agentdash-application/src/session/hook_delegate.rs:580`，后续构造 injected messages 的路径从 `crates/agentdash-application/src/session/hook_delegate.rs:589` 开始。
- `HookRuntimeDelegate::after_turn` 定义在 `crates/agentdash-application/src/session/hook_delegate.rs:729` 到 `crates/agentdash-application/src/session/hook_delegate.rs:759`，对 `HookTrigger::AfterTurn` 求值的位置是 `crates/agentdash-application/src/session/hook_delegate.rs:734` 到 `crates/agentdash-application/src/session/hook_delegate.rs:746`。
- 当前 `HookRuntimeDelegate::after_turn` 返回空 `steering` 和空 `follow_up`，位置是 `crates/agentdash-application/src/session/hook_delegate.rs:749` 到 `crates/agentdash-application/src/session/hook_delegate.rs:752`。
- `HookRuntimeDelegate::before_stop` 定义在 `crates/agentdash-application/src/session/hook_delegate.rs:762` 到 `crates/agentdash-application/src/session/hook_delegate.rs:814`，对 `HookTrigger::BeforeStop` 求值的位置是 `crates/agentdash-application/src/session/hook_delegate.rs:767` 到 `crates/agentdash-application/src/session/hook_delegate.rs:778`。
- `HookRuntimeDelegate::before_stop` 读取 unresolved pending actions，位置是 `crates/agentdash-application/src/session/hook_delegate.rs:781`；在无未解决动作且 completion satisfied 时返回 stop，位置是 `crates/agentdash-application/src/session/hook_delegate.rs:789` 到 `crates/agentdash-application/src/session/hook_delegate.rs:798`。
- `HookRuntimeDelegate::before_stop` 当前 Continue 分支返回空 `steering` 和空 `follow_up`，`allow_empty = true`，位置是 `crates/agentdash-application/src/session/hook_delegate.rs:801` 到 `crates/agentdash-application/src/session/hook_delegate.rs:814`。
- runtime injection 通过 `RuntimeInjectionSource::Hook(trigger)` 发送到 runtime sink，位置是 `crates/agentdash-application/src/session/hook_delegate.rs:226` 到 `crates/agentdash-application/src/session/hook_delegate.rs:235`。
- `after_turn_routes_hook_injections_through_runtime_sink` 测试在 `crates/agentdash-application/src/session/hook_delegate.rs:1907` 到 `crates/agentdash-application/src/session/hook_delegate.rs:1950` 说明 AfterTurn injection 走 runtime sink，而不是 inline steering。
- `HookRuntimeAccess` 定义在 `crates/agentdash-spi/src/hooks/mod.rs:482` 到 `crates/agentdash-spi/src/hooks/mod.rs:490`，暴露 `session_id()` 和 `control_target()`。
- `HookControlTarget { run_id, agent_id, frame_id }` 定义在 `crates/agentdash-spi/src/hooks/mod.rs:560` 到 `crates/agentdash-spi/src/hooks/mod.rs:564`。
- `RuntimeAdapterProvenance { runtime_session_id, turn_id, source }` 定义在 `crates/agentdash-spi/src/hooks/mod.rs:568` 到 `crates/agentdash-spi/src/hooks/mod.rs:573`。
- `HookPendingAction` 定义在 `crates/agentdash-spi/src/hooks/mod.rs:186` 到 `crates/agentdash-spi/src/hooks/mod.rs:208`，`is_follow_up` helper 在 `crates/agentdash-spi/src/hooks/mod.rs:474` 到 `crates/agentdash-spi/src/hooks/mod.rs:476`。
- 结论：`decision.steering`、`decision.follow_up`、`pending_follow_up_messages` 是 Agent loop 支持的传输缓冲，但当前 `HookRuntimeDelegate` 没有实际把 hook 输出写入这些字段。UserPromptSubmit block/rewrite/context injection 属于 hook runtime 策略/上下文注入，不是 AgentRun anchored delivery。AfterTurn / BeforeStop 若要成为 AgentRun anchored delivery，应新增 adapter：从 hook result / pending action 生成 `MailboxMessageOrigin::Hook` envelope，并使用 `HookRuntimeAccess.control_target()`、`session_id()`、`RuntimeAdapterProvenance.turn_id` 绑定 run / agent / frame / runtime session / turn。

### Runtime Delegate Assembly

- launch planner 在 `crates/agentdash-application/src/session/launch/planner.rs:145` 到 `crates/agentdash-application/src/session/launch/planner.rs:157` 创建 `runtime_delegate`。
- `HookRuntimeDelegate::new_with_mount_root_audit_and_sink(...)` 调用在 `crates/agentdash-application/src/session/launch/planner.rs:151` 到 `crates/agentdash-application/src/session/launch/planner.rs:156`。
- `runtime_delegate` 放入 `LaunchPlanInput` 的位置是 `crates/agentdash-application/src/session/launch/planner.rs:245`。
- `LaunchPlan.runtime_delegate` 字段在 `crates/agentdash-application/src/session/launch/plan.rs:145`。
- `ExecutionTurnFrame { runtime_delegate: input.runtime_delegate }` 构造在 `crates/agentdash-application/src/session/launch/plan.rs:271` 到 `crates/agentdash-application/src/session/launch/plan.rs:275`。
- `ExecutionTurnFrame.runtime_delegate` 字段在 `crates/agentdash-spi/src/connector/mod.rs:97` 到 `crates/agentdash-spi/src/connector/mod.rs:100`。
- `Agent::set_runtime_delegate` 在 `crates/agentdash-agent/src/agent.rs:173` 到 `crates/agentdash-agent/src/agent.rs:174`。
- `AgentLoopConfig.runtime_delegate` 在 `crates/agentdash-agent/src/agent.rs:481` 填充。
- 结论：scheduler 若选择 runtime delegate 路径接入，最小侵入的组装点是 launch planner。可以构造 composite `AgentRuntimeDelegate`，先保留 `HookRuntimeDelegate` 的 hook runtime 策略，再在 AfterTurn / BeforeStop 边界触发 mailbox scheduler 并把 claimed steering/follow_up 汇入返回 decision。

### HookAutoResume Current Call Chain and Anchors

- `TerminalEffectExecutor::HookAutoResume` 定义在 `crates/agentdash-application/src/session/terminal_effects.rs:29`。
- `TerminalAutoResumePort::request_hook_auto_resume` trait 边界定义在 `crates/agentdash-application/src/session/terminal_effects.rs:89` 到 `crates/agentdash-application/src/session/terminal_effects.rs:90`。
- `enqueue_terminal_effects` 写入 `TerminalEffectType::HookEffects` 的位置是 `crates/agentdash-application/src/session/terminal_effects.rs:142` 到 `crates/agentdash-application/src/session/terminal_effects.rs:149`。
- `enqueue_terminal_effects` 写入 `TerminalEffectType::SessionTerminalCallback` 的位置是 `crates/agentdash-application/src/session/terminal_effects.rs:158` 到 `crates/agentdash-application/src/session/terminal_effects.rs:174`。
- `enqueue_terminal_effects` 在 `should_auto_resume(...)` 为 true 时写入 `TerminalEffectType::HookAutoResume`，位置是 `crates/agentdash-application/src/session/terminal_effects.rs:178` 到 `crates/agentdash-application/src/session/terminal_effects.rs:190`。
- `insert_terminal_effect` 定义在 `crates/agentdash-application/src/session/terminal_effects.rs:204` 到 `crates/agentdash-application/src/session/terminal_effects.rs:208`。
- terminal effect replay 把 `TerminalEffectType::HookAutoResume` 映射到 `TerminalEffectExecutor::HookAutoResume`，位置是 `crates/agentdash-application/src/session/terminal_effects.rs:265` 到 `crates/agentdash-application/src/session/terminal_effects.rs:267`。
- terminal effect execute 调用 `deps.auto_resume.request_hook_auto_resume(item.record.session_id.clone())`，位置是 `crates/agentdash-application/src/session/terminal_effects.rs:348` 到 `crates/agentdash-application/src/session/terminal_effects.rs:353`。
- effect 成功状态标记在 `crates/agentdash-application/src/session/terminal_effects.rs:360` 到 `crates/agentdash-application/src/session/terminal_effects.rs:364`。
- `request_hook_auto_resume(session_id)` 定义在 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:188`。
- `request_hook_auto_resume` 使用 `runtime_registry.increment_auto_resume_if_allowed` 做 session-local cap，位置是 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:191` 到 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:195`。
- 允许 auto-resume 后调用 `schedule_hook_auto_resume(session_id)`，位置是 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:202`。
- `schedule_hook_auto_resume` 定义在 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:221` 到 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:247`。
- `schedule_hook_auto_resume` 创建 `LaunchCommand::hook_auto_resume_input(UserPromptInput::from_text(msg::AUTO_RESUME_PROMPT))`，位置是 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:225` 到 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:227`。
- `schedule_hook_auto_resume` 发送 `auto_resume` context frame，位置是 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:228` 到 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:232`。
- `schedule_hook_auto_resume` 直接调用 `hub.launch_service().launch_command(&session_id, command)`，位置是 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:235` 到 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:238`。
- `LaunchSource::HookAutoResume` 定义在 `crates/agentdash-application/src/session/launch/command.rs:11` 和 `crates/agentdash-application/src/session/launch/command.rs:92`，`hook_auto_resume_input` constructor 在 `crates/agentdash-application/src/session/launch/command.rs:139` 到 `crates/agentdash-application/src/session/launch/command.rs:140`。
- `RuntimeSessionExecutionAnchor` 结构定义在 `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:29` 到 `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:33`，包含 `runtime_session_id`、`run_id`、`launch_frame_id`、`agent_id`。
- `RuntimeSessionExecutionAnchor::new_dispatch` 在 `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:47` 到 `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:55`。
- `RuntimeSessionExecutionAnchorRepository::find_by_session` trait 定义在 `crates/agentdash-domain/src/workflow/repository.rs:139` 到 `crates/agentdash-domain/src/workflow/repository.rs:142`，Postgres 实现在 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:834` 到 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:842`。
- 结论：HookAutoResume 当前只有 `session_id` 进入 `request_hook_auto_resume` / `schedule_hook_auto_resume` direct launch；但 terminal effect record、runtime session anchor repository、hook runtime access 都能提供写 mailbox envelope 需要的 anchor。迁移时应在 `TerminalEffectExecutor::HookAutoResume` 执行阶段或 effect enqueue 阶段创建 hook-origin mailbox message，用 terminal effect id 或 `{session_id}:{turn_id}:{terminal_event_seq}:hook_auto_resume` 作为 dedup key，然后由 scheduler 以 `LaunchSource::HookAutoResume` delivery。当前 in-memory `increment_auto_resume_if_allowed` 不应成为唯一幂等来源。

### Terminal Callback, Failed / Interrupted Pause, and Pending Queue

- `PendingQueueService` 是 in-memory `HashMap`，字段定义在 `crates/agentdash-application/src/session/pending_queue.rs:71` 到 `crates/agentdash-application/src/session/pending_queue.rs:75`。
- `PendingQueueService::enqueue` 定义在 `crates/agentdash-application/src/session/pending_queue.rs:85` 到 `crates/agentdash-application/src/session/pending_queue.rs:105`。
- `PendingQueueService::list` 定义在 `crates/agentdash-application/src/session/pending_queue.rs:108` 到 `crates/agentdash-application/src/session/pending_queue.rs:114`。
- `PendingQueueService::delete` 定义在 `crates/agentdash-application/src/session/pending_queue.rs:117` 到 `crates/agentdash-application/src/session/pending_queue.rs:126`。
- `PendingQueueService::dequeue_front` 定义在 `crates/agentdash-application/src/session/pending_queue.rs:129` 到 `crates/agentdash-application/src/session/pending_queue.rs:142`，pause 时直接返回 none 的位置是 `crates/agentdash-application/src/session/pending_queue.rs:131` 到 `crates/agentdash-application/src/session/pending_queue.rs:134`。
- `PendingQueueService::take` 定义在 `crates/agentdash-application/src/session/pending_queue.rs:155` 到 `crates/agentdash-application/src/session/pending_queue.rs:165`。
- `PendingQueueService::pause` / `resume` / `is_paused` 分别在 `crates/agentdash-application/src/session/pending_queue.rs:169` 到 `crates/agentdash-application/src/session/pending_queue.rs:188`。
- `AgentRunPendingDispatcher` 定义在 `crates/agentdash-api/src/agent_run_pending.rs:14` 到 `crates/agentdash-api/src/agent_run_pending.rs:18`。
- `AgentRunPendingDispatcher::dispatch_next_pending` 定义在 `crates/agentdash-api/src/agent_run_pending.rs:33` 到 `crates/agentdash-api/src/agent_run_pending.rs:66`，dequeue 后构造 `AgentRunMessageService` 发送 next-turn message。
- `AgentRunPendingDispatcher::resume_queue` 定义在 `crates/agentdash-api/src/agent_run_pending.rs:68` 到 `crates/agentdash-api/src/agent_run_pending.rs:83`，失败时重新 pause。
- `AgentRunPendingDispatcher::pause_queue` 定义在 `crates/agentdash-api/src/agent_run_pending.rs:85` 到 `crates/agentdash-api/src/agent_run_pending.rs:87`。
- `AgentRunPendingTerminalCallback` 的 `SessionTerminalCallback` 实现在 `crates/agentdash-api/src/agent_run_pending.rs:102` 到 `crates/agentdash-api/src/agent_run_pending.rs:126`。
- `AgentRunPendingTerminalCallback` 在 `"completed"` 时 dispatch next pending，位置是 `crates/agentdash-api/src/agent_run_pending.rs:105` 到 `crates/agentdash-api/src/agent_run_pending.rs:112`。
- `AgentRunPendingTerminalCallback` 在 `"failed"` 时 pause 为 `TurnFailed`，位置是 `crates/agentdash-api/src/agent_run_pending.rs:114` 到 `crates/agentdash-api/src/agent_run_pending.rs:117`。
- `AgentRunPendingTerminalCallback` 在 `"interrupted"` 时 pause 为 `TurnInterrupted`，位置是 `crates/agentdash-api/src/agent_run_pending.rs:119` 到 `crates/agentdash-api/src/agent_run_pending.rs:122`。
- `pending_message_command` 在 `crates/agentdash-api/src/agent_run_pending.rs:129` 到 `crates/agentdash-api/src/agent_run_pending.rs:136` 生成 `pending:{id}:{uuid}` client command id，因此 pending dispatch receipt 不是 stable user command id。
- `AgentRunPendingDispatcher` 在 API bootstrap 构造，位置是 `crates/agentdash-api/src/bootstrap/session.rs:171` 到 `crates/agentdash-api/src/bootstrap/session.rs:175`。
- `AgentRunPendingTerminalCallback` wrapper 在 `crates/agentdash-api/src/bootstrap/session.rs:176` 构造，并在 `crates/agentdash-api/src/bootstrap/session.rs:178` 到 `crates/agentdash-api/src/bootstrap/session.rs:180` 与 orchestrator terminal callback 组成 composite callback。
- `CompositeSessionTerminalCallback::on_session_terminal` 在 `crates/agentdash-api/src/bootstrap/session.rs:218` 到 `crates/agentdash-api/src/bootstrap/session.rs:224` 顺序调用 callback。
- `TurnEvent::Terminal { kind, message }` 定义在 `crates/agentdash-application/src/session/turn_processor.rs:21` 到 `crates/agentdash-application/src/session/turn_processor.rs:28`。
- turn processor 记录 terminal event 的位置是 `crates/agentdash-application/src/session/turn_processor.rs:104` 到 `crates/agentdash-application/src/session/turn_processor.rs:108`，持久化 terminal notification 的位置是 `crates/agentdash-application/src/session/turn_processor.rs:132` 到 `crates/agentdash-application/src/session/turn_processor.rs:135`。
- turn processor 在 dispatch terminal effects 前清 active turn，位置是 `crates/agentdash-application/src/session/turn_processor.rs:137` 到 `crates/agentdash-application/src/session/turn_processor.rs:142`。
- terminal effects dispatch 在 `crates/agentdash-application/src/session/turn_processor.rs:170` 到 `crates/agentdash-application/src/session/turn_processor.rs:175`。
- `SessionExecutionState` 定义在 `crates/agentdash-application/src/session/types.rs:220` 到 `crates/agentdash-application/src/session/types.rs:239`；`SessionCoreService::inspect_session_execution_state` 定义在 `crates/agentdash-application/src/session/core.rs:150` 到 `crates/agentdash-application/src/session/core.rs:177`。
- `meta_to_execution_state` 定义在 `crates/agentdash-application/src/session/hub_support.rs:347` 到 `crates/agentdash-application/src/session/hub_support.rs:385`，在没有 runtime entry 时把 `ExecutionStatus::Running` 映射为 `Interrupted` 的逻辑在 `crates/agentdash-application/src/session/hub_support.rs:375` 到 `crates/agentdash-application/src/session/hub_support.rs:383`。
- 结论：terminal callback 是旧 pending queue completed-drain 和 failed/interrupted-pause 的当前入口。迁移到 scheduler trigger 时，应替换 `AgentRunPendingTerminalCallback` / `AgentRunPendingDispatcher`：completed 触发 scheduler 的 terminal fallback drain；failed/interrupted pause mailbox state；manual resume endpoint 改为 mailbox resume state + schedule。由于 active turn 在 terminal effects 前已清空，terminal callback 不能继续当前 agent loop，只能触发 next launch/fallback；继续当前 loop 的位置必须是 BeforeStop 或 AgentLoopTurn runtime delegate 边界。

### Existing Mailbox Foundation and Gaps

- mailbox domain model 已存在于 `crates/agentdash-domain/src/workflow/mailbox.rs`。
- `MailboxMessageOrigin` 定义在 `crates/agentdash-domain/src/workflow/mailbox.rs:8`，`MailboxMessageSource` 在 `crates/agentdash-domain/src/workflow/mailbox.rs:46`，`MailboxDelivery` 在 `crates/agentdash-domain/src/workflow/mailbox.rs:115`。
- `ConsumptionBarrier` 定义在 `crates/agentdash-domain/src/workflow/mailbox.rs:176`，`MailboxDrainMode` 在 `crates/agentdash-domain/src/workflow/mailbox.rs:211`，`MailboxMessageStatus` 在 `crates/agentdash-domain/src/workflow/mailbox.rs:240`。
- `AgentRunMailboxMessage` 定义在 `crates/agentdash-domain/src/workflow/mailbox.rs:293`，`NewAgentRunMailboxMessage` 在 `crates/agentdash-domain/src/workflow/mailbox.rs:330`，`AgentRunMailboxState` 在 `crates/agentdash-domain/src/workflow/mailbox.rs:352`，`AgentRunMailboxClaimRequest` 在 `crates/agentdash-domain/src/workflow/mailbox.rs:363`。
- `AgentRunMailboxRepository` trait 定义在 `crates/agentdash-domain/src/workflow/mailbox.rs:375`。
- migration `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql` 在 `:11` 给 command receipts 增加 `mailbox_message_id`，在 `:34` 创建 `agent_run_mailbox_messages`，在 `:133` 创建 mailbox state，相关约束/索引集中在 `:69` 到 `:153`。
- Postgres repository 的 `claim_next` 在 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:176` 到 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:222`。
- `recover_expired_consuming` 在 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:224` 到 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:237`。
- `mark_message_status` 在 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:239` 到 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:274`。
- `delete_message` 在 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:276` 到 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:294`。
- `cleanup_user_payload` 在 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:296` 到 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:308`。
- `pause_state` 在 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:310` 到 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:338`，`resume_state` 在 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:341` 到 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:357`。
- `MailboxMessageView` 定义在 `crates/agentdash-contracts/src/workflow.rs:1157` 到 `crates/agentdash-contracts/src/workflow.rs:1178`，`MailboxStateView` 在 `crates/agentdash-contracts/src/workflow.rs:1182` 到 `crates/agentdash-contracts/src/workflow.rs:1190`。
- `AgentRunWorkspaceView` 仍包含 `pending_queue` / `pending_messages`，位置是 `crates/agentdash-contracts/src/workflow.rs:1243` 到 `crates/agentdash-contracts/src/workflow.rs:1245`。
- `AgentRunComposerSubmitResponse` 仍是 old shape，定义在 `crates/agentdash-contracts/src/workflow.rs:1268` 到 `crates/agentdash-contracts/src/workflow.rs:1279`。
- `AgentRunMessageCommandResponse` new shape 已存在，定义在 `crates/agentdash-contracts/src/workflow.rs:1308` 到 `crates/agentdash-contracts/src/workflow.rs:1319`。
- `SessionRuntimeControlView` 仍包含 pending fields，位置是 `crates/agentdash-contracts/src/workflow.rs:1554` 到 `crates/agentdash-contracts/src/workflow.rs:1556`；`PendingMessageView` 定义在 `crates/agentdash-contracts/src/workflow.rs:1561` 到 `crates/agentdash-contracts/src/workflow.rs:1565`。
- 未找到 `AgentRunMailboxService`、`MailboxScheduler`、`accept_user_message`、`accept_hook_message`、`agent_loop_turn_boundary`、`ConsumptionBarrier::AgentLoopTurnBoundary` 在 application/API/agent 层的实际调用。
- 结论：storage/domain/contract 已有一部分 mailbox 基础，但 scheduler service、API 接入、agent loop boundary、hook-origin envelope 生成、terminal fallback drain 还没有落地。

### Frontend and Contract Surface

- `AgentRunWorkspacePage` 仍从 lifecycle service 引入 `submitAgentRunComposerInput`、`deleteAgentRunPendingMessage`、`promoteAgentRunPendingMessage`、`resumeAgentRunPendingQueue`，位置是 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:29` 到 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:33`。
- composer submit 调用在 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:496` 到 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:501`。
- 页面仍读取 `accepted_kind === "steer"` 的旧 response 行为，位置约在 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:506`。
- runtime control 仍传入 `pendingMessages={runtimeControl?.pending_messages}`，位置约在 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:919`。
- pending promote/delete/resume handlers 仍传给组件，位置约在 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:921` 到 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:923`。
- `submitAgentRunComposerInput` service 定义在 `packages/app-web/src/services/lifecycle.ts:79` 到 `packages/app-web/src/services/lifecycle.ts:87`。
- pending endpoint tests 在 `packages/app-web/src/services/lifecycle.test.ts:87` 到 `packages/app-web/src/services/lifecycle.test.ts:100`。
- 结论：前端 surface 仍围绕 pending queue 和 old composer response。Mailbox scheduler 落地时，需要把 workspace/runtime control projection 切到 `MailboxMessageView` / `MailboxStateView`，composer submit 返回 `AgentRunMessageCommandResponse`，pending endpoints 改名或替换为 mailbox promote/delete/resume。

### Suggested Write Set / Executable Checklist

1. 新增 application 层 mailbox service/scheduler
   - 建议文件：`crates/agentdash-application/src/workflow/agent_run_mailbox.rs`。
   - 建议结构：`AgentRunMailboxService`、`MailboxScheduler`、`MailboxSchedulerTrigger`。
   - trigger 建议包含：`UserMessageSubmitted`、`PromoteRequested`、`ManualResume`、`AgentLoopTurnBoundary`、`BeforeStopAgentRunTurnBoundary`、`TerminalAgentRunTurnBoundary`、`TerminalFailedOrInterrupted`、`HookAutoResumeEffect`。
   - service 职责：写 `NewAgentRunMailboxMessage`、dedup by source key / client command id、claim eligible messages、调用 launch/steer adapters、mark delivered/failed/deleted、pause/resume state、cleanup payload。

2. 重用并收敛 delivery adapters
   - idle/next-turn launch 可复用 `AgentRunMessageService` 和 `AgentRunMessageLaunchDeliveryPort` 的 transport，但要把 delivery 放到 scheduler claim 后执行。
   - active-turn steering 可复用 `AgentRunSteeringService` / `SessionControlService::steer_session` 逻辑，但应由 scheduler 根据 `SessionExecutionState::Running { active_turn_id }`、`supports_session_steering` 和 message delivery mode 决定是否 steer。
   - delivery 结果应写回 mailbox message：`accepted_agent_run_turn_id`、`protocol_turn_id`、`delivery_runtime_session_id`、`result_json`、`failure_json`，并把 `agent_run_command_receipts.mailbox_message_id` 补齐。

3. 替换 composer submit 分支
   - 修改点：`crates/agentdash-api/src/routes/lifecycle_agents.rs::submit_agent_run_composer_input` 和 `classify_composer_submit_kind`。
   - 新流程：route 校验输入后调用 `AgentRunMailboxService::accept_user_message(...)`，再调用 scheduler；由 scheduler 决定 launch、steer、enqueue 或 paused。
   - 删除/弃用 route 层 `accepted_receipt` synthetic response，改返回 `AgentRunMessageCommandResponse`。

4. AgentLoopTurn boundary 接入 scheduler
   - 修改点：`crates/agentdash-agent/src/agent_loop.rs`、`crates/agentdash-agent-types/src/runtime/delegate.rs`、`crates/agentdash-application/src/session/launch/planner.rs`。
   - 由于 `poll_steering` 是同步 callback，DB scheduler 不宜接入 `GetMessagesFn`。建议在 `run_after_turn_delegate` 之后、`poll_steering` 之前通过 async delegate 触发 `AgentLoopTurnBoundary`，将 claimed mailbox messages 作为 `TurnControlDecision.steering` 汇入 `pending_messages`。
   - `QueueMode::All` 的 durable 等价语义应落在 `MailboxDrainMode` / `claim_next` 或批量 claim 规则上，不依赖 in-memory `std::mem::take(queue)`。

5. BeforeStop boundary 接入 scheduler
   - 修改点：`HookRuntimeDelegate::before_stop` 或 composite `AgentRuntimeDelegate`。
   - 目的：在 terminal event 之前、agent loop 仍可继续时 drain mailbox。scheduler 若返回 steering/follow_up，则生成 `StopDecision::Continue` 或合并到现有 Continue decision。
   - 当前 hook runtime 的 BeforeStop gate 仍应保留：`unresolved_pending_actions` 和 completion satisfied 判定属于 hook runtime 策略，不是 mailbox delivery。

6. Hook-origin envelope intake
   - 修改点：`crates/agentdash-application/src/session/hook_delegate.rs` 及 hook runtime adapter 周边。
   - AfterTurn / BeforeStop hook 产生的可投递 follow-up/steering，应写 `MailboxMessageOrigin::Hook` envelope，并以 `HookRuntimeAccess.control_target()`、`session_id()`、`RuntimeAdapterProvenance.turn_id` 绑定 anchor。
   - `UserPromptSubmit` block/rewrite/context injection 保持 runtime strategy/context injection，不写 mailbox，除非 hook 明确创建 follow-up command。

7. HookAutoResume 迁移
   - 修改点：`crates/agentdash-application/src/session/terminal_effects.rs`、`crates/agentdash-application/src/session/hub/hook_dispatch.rs`。
   - 建议把 `TerminalEffectExecutor::HookAutoResume` 从 direct `request_hook_auto_resume` / `schedule_hook_auto_resume` 改成写 hook-origin mailbox message + scheduler trigger。
   - dedup key 建议用 terminal effect id，或 `{runtime_session_id}:{turn_id}:{terminal_event_seq}:hook_auto_resume`。
   - delivery 仍可使用 `LaunchSource::HookAutoResume`，但通过 scheduler launch adapter 发起。

8. Terminal callback / pause 迁移
   - 修改点：`crates/agentdash-api/src/agent_run_pending.rs`、`crates/agentdash-api/src/bootstrap/session.rs`。
   - 用 `AgentRunMailboxTerminalCallback` 替代 `AgentRunPendingTerminalCallback`。
   - completed：触发 scheduler 的 terminal fallback drain。
   - failed/interrupted：调用 mailbox repository `pause_state`，reason 对应 terminal kind。
   - manual resume endpoint：调用 `resume_state` 后触发 scheduler。

9. Projection / API / frontend 替换
   - 修改点：`crates/agentdash-contracts/src/workflow.rs`、workspace view assembler、`packages/app-web/src/pages/AgentRunWorkspacePage.tsx`、`packages/app-web/src/services/lifecycle.ts`、相关 tests。
   - workspace/runtime control 应暴露 `MailboxMessageView` / `MailboxStateView`，不再以 pending queue 为 primary surface。
   - promote/delete/resume API 改为 mailbox message/state API；前端 pending 控件改读 mailbox projection。

10. Tests
   - scheduler unit/integration：idle launch、running steer、running no-steering enqueue、AgentLoopTurnBoundary drain all、BeforeStop continuation、terminal completed fallback、failed/interrupted pause、manual resume、hook auto-resume dedup。
   - API tests：duplicate client command id、mailbox message id 写入 receipt、composer submit response、mailbox promote/delete/resume。
   - Frontend tests：new response shape、mailbox projection rendering、pending endpoint replacement。

## Caveats / Not Found

- 未找到 application/API 层已存在的 `AgentRunMailboxService` 或 `MailboxScheduler`；当前只有 domain/repository/migration/contract 的 mailbox 基础。
- 未找到 `AgentRunMailboxRepository::claim_next` 在 runtime/API 调度路径中的调用；mailbox 目前尚未成为 delivery source of truth。
- 当前 `HookRuntimeDelegate::after_turn` 和 `before_stop` 不产生实际 `decision.steering` / `decision.follow_up`，只保留 trait/agent loop 传输能力和 hook runtime gate/injection 行为。
- `poll_steering` / `GetMessagesFn` 是同步函数，这是 DB-backed scheduler 接入 AgentLoopTurn 的主要边界限制。
- terminal effects dispatch 前 active turn 已清空；terminal callback 只能做 fallback launch/drain，不能继续当前 agent loop。要在当前 loop 内继续，必须在 BeforeStop 或 TurnEnd 后 runtime delegate 边界处理。
- HookAutoResume 当前 direct launch 只接收 `session_id`，锚点需要通过 terminal effect record、runtime session anchor repository 或 hook runtime access 补齐。
- 本研究未使用外部网络资料；所有结论来自本仓库源码、migration、contract 和 Trellis spec。
