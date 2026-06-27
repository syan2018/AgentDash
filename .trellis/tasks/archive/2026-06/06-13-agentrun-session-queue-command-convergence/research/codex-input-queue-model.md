# Research: Codex input queue model

- Query: `references/codex` 中和会话输入队列、pending input、interrupt-steer、resume、turn boundary、hooks，以及 turn/session/run 生命周期命名相关的模型与实现
- Scope: internal
- Date: 2026-06-13

## Findings

### Files found

- `references/codex/codex-rs/core/src/session/input_queue.rs` - Codex 核心输入队列，区分 active turn 的 pending input 与 session-scoped mailbox。
- `references/codex/codex-rs/core/src/state/turn.rs` - active turn 状态，定义 `MailboxDeliveryPhase::CurrentTurn | NextTurn`。
- `references/codex/codex-rs/core/src/session/turn.rs` - 模型循环如何在内部边界 drain pending input/mailbox、执行 hooks、处理 auto-compact 与 follow-up。
- `references/codex/codex-rs/core/src/session/handlers.rs` - `Op::UserInput` 的 steer-or-new-turn 分发，以及 inter-agent mail 入队。
- `references/codex/codex-rs/core/src/tasks/mod.rs` - idle 时根据 trigger-turn mailbox work 启动新 turn，并在 interrupt 后清理/重新尝试 pending work。
- `references/codex/codex-rs/protocol/src/protocol.rs` - `InterAgentCommunication`、turn status、non-steerable turn error 等协议定义。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs` - v2 `TurnSteerParams` 要求 `expected_turn_id`，并返回 accepted `turn_id`。
- `references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs` - steer/interrupt API 的 active turn 校验、错误映射、analytics 记录。
- `references/codex/codex-rs/analytics/src/reducer.rs` - pending steer 请求到 accepted/rejected telemetry 的归约。
- `references/codex/codex-rs/hooks/src/events/user_prompt_submit.rs` - UserPromptSubmit hook 的 block/additional-context 语义。
- `references/codex/codex-rs/hooks/src/events/stop.rs` - Stop hook 的 block/continue_false/continuation prompt 聚合语义。
- `references/codex/codex-rs/core/src/hook_runtime.rs` - hooks 与 pending input record 的连接点；只对 `TurnInput::UserInput` 执行 UserPromptSubmit。
- `references/codex/codex-rs/tui/src/chatwidget/input_queue.rs` - TUI 侧 queued user messages、pending steers、rejected steers 的展示状态。
- `references/codex/codex-rs/tui/src/chatwidget/input_flow.rs` - TUI 何时立即提交、何时排队，以及 idle 后只发一个普通 queued input。
- `references/codex/codex-rs/tui/src/chatwidget/input_submission.rs` - TUI 运行中输入生成 pending steer。
- `references/codex/codex-rs/tui/src/chatwidget/interaction.rs` - interrupt 与 pending steer 的联动。
- `references/codex/codex-rs/tui/src/interrupts.rs` - approval/elicitation/user-input 等 interrupt prompts 的 UI 队列，独立于 pending user messages。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs` - thread resume 协议，偏向线程重连/投影恢复。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs` - app-server 公共 `Thread` / `Turn` read model。
- `references/codex/codex-rs/core/src/session/session.rs` - live `Session` runtime，持有 active turn 和 input queue。
- `references/codex/codex-rs/core/src/session/turn_context.rs` - 单个 thread turn 的运行上下文。
- `references/codex/codex-rs/core/src/tasks/user_shell.rs` - 区分独立 turn lifecycle 与 active-turn auxiliary work。
- `references/codex/codex-rs/app-server/src/request_processors/thread_lifecycle.rs` - running thread resume 时恢复 active turn snapshot 与 turns page。
- `references/codex/sdk/python/src/openai_codex/_goal.py` - Python SDK 将多个物理 runtime continuation 聚合成一个 logical turn stream。
- `references/codex/sdk/python/src/openai_codex/api.py` - Python SDK 暴露 `Thread.run()` / `Thread.turn()` / `TurnHandle`。
- `references/codex/sdk/python/tests/test_app_server_goal_operations.py` - goal operation 测试证明多个 runtime continuation 可投影成一个 logical turn。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/hook.rs` - `HookRunStatus` / `HookRunSummary`，说明 `Run` 在 Codex 中也用于 hook 执行实例。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs` - multi-agent message tool 用 `QueueOnly | TriggerTurn` 映射 `trigger_turn`。
- `references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs` - wait 工具订阅 mailbox watch 作为唤醒源。

### Related specs and task docs

- `.trellis/workflow.md` - research 必须持久化到 task `research/`，planning 阶段通过文档与研究收敛。
- `.trellis/spec/backend/index.md` - backend spec 索引。
- `.trellis/spec/backend/session/architecture.md` - Session 是 RuntimeSession，AgentRun 命令使用 AgentRun public identity；终态事实先落库，副作用走 durable outbox。
- `.trellis/spec/backend/session/runtime-execution-state.md` - runtime active turn、pending runtime commands、terminal outbox、workspace command 的现状边界。
- `.trellis/spec/backend/hooks/architecture.md` - hook control decisions 在 loop boundary 生效，UserPromptSubmit 可动态注入文本。
- `.trellis/spec/backend/hooks/execution-hook-runtime.md` - Hook runtime 与 delivery 分离；auto-resume/context frame 应明确建模。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md` - 前端现有 pending/resume/promote/delete API 与 run/workspace identity 投影。
- `.trellis/tasks/06-13-agentrun-session-queue-command-convergence/prd.md` - 本任务目标是统一 User/System/Hook/Follow-up/Pending 为 durable mailbox envelope。
- `.trellis/tasks/06-13-agentrun-session-queue-command-convergence/design.md` - 已规划 `MessageEnvelope`、barrier、drain mode、scheduler、hook convergence、recovery。
- `.trellis/tasks/06-13-agentrun-session-queue-command-convergence/current-state.md` - 当前实现仍存在 route-local classification、in-memory pending queue、terminal drain、hook bypass 等切换点。

### Codex model key points

Codex 把输入分成两层：active turn 内部的 pending input，以及 session-scoped mailbox。`TurnInput` 包含 `UserInput`、`ResponseItem`、`InterAgentCommunication`，`TurnInputQueue` 是 turn-local，而 `InputQueue` 额外维护 session mailbox 与 watch 唤醒器（`references/codex/codex-rs/core/src/session/input_queue.rs:12`, `references/codex/codex-rs/core/src/session/input_queue.rs:22`, `references/codex/codex-rs/core/src/session/input_queue.rs:28`）。mailbox 入队只 push 到 `VecDeque` 并通知 watch，是否进入当前 turn 由 active turn 的 delivery phase 决定（`references/codex/codex-rs/core/src/session/input_queue.rs:51`, `references/codex/codex-rs/core/src/session/input_queue.rs:74`, `references/codex/codex-rs/core/src/session/input_queue.rs:173`）。

Codex 用 `MailboxDeliveryPhase::CurrentTurn | NextTurn` 保护 turn boundary。注释明确说：当前 turn 可以消费 mailbox，直到记录了用户可见终端输出；之后切到 `NextTurn`，避免 late child mail 插入已经准备结束的当前 turn，除非显式 same-turn work 重新打开 current-turn delivery（`references/codex/codex-rs/core/src/state/turn.rs:39`, `references/codex/codex-rs/core/src/state/turn.rs:47`, `references/codex/codex-rs/core/src/session/input_queue.rs:105`, `references/codex/codex-rs/core/src/session/input_queue.rs:121`）。

`get_pending_input` 是关键原子边界：先 split active turn pending input；只有 `accepts_mailbox_delivery_for_current_turn` 为 true 时才 drain mailbox，并且 mailbox item 排在 pending input 后面（`references/codex/codex-rs/core/src/session/input_queue.rs:173`, `references/codex/codex-rs/core/src/session/input_queue.rs:185`, `references/codex/codex-rs/core/src/session/input_queue.rs:196`）。这给 AgentRun 的 drain mode 一个直接启发：drain 需要绑定 active internal turn 的可接收 phase，不能只看 session 是否 running。

Codex 的 pending input 不会在所有边界立即抢占模型。`session/turn.rs` 在 turn 开始先让新输入采样；drain pending input 只在 `can_drain_pending_input` 为 true 时发生；auto-compact 或模型 continuation 后还会延迟 pending input，避免 steer 抢在必要的 continuation 前面（`references/codex/codex-rs/core/src/session/turn.rs:168`, `references/codex/codex-rs/core/src/session/turn.rs:199`, `references/codex/codex-rs/core/src/session/turn.rs:204`, `references/codex/codex-rs/core/src/session/turn.rs:248`, `references/codex/codex-rs/core/src/session/turn.rs:296`）。测试覆盖了 steered user input 在 mid-turn compact 与 tool-output compact 后等待模型 continuation 的行为（`references/codex/codex-rs/core/tests/suite/pending_input.rs:501`, `references/codex/codex-rs/core/tests/suite/pending_input.rs:685`）。

Codex 对 inter-agent mail 有一种非常粗粒度的唤醒标记：`InterAgentCommunication.trigger_turn`。入队后如果 `trigger_turn` 为 true，会尝试在 idle session 上启动 turn；但如果当前 turn 已在 answer boundary 后，则 trigger-turn mail 留在队列给下个 turn（`references/codex/codex-rs/protocol/src/protocol.rs:670`, `references/codex/codex-rs/core/src/session/handlers.rs:276`, `references/codex/codex-rs/core/src/tasks/mod.rs:446`, `references/codex/codex-rs/core/src/tasks/mod.rs:457`, `references/codex/codex-rs/core/src/session/tests.rs:9251`）。multi-agent message tool 只把 delivery mode 映射为 `QueueOnly => false`、`TriggerTurn => true`（`references/codex/codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs:17`）。

Codex steer API 强绑定 active turn id。v2 `TurnSteerParams` 要求 `expected_turn_id`，server 在处理时校验 active turn 是否存在、是否匹配、是否 steerable；accepted response 返回实际 accepted `turn_id`（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:165`, `references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs:782`, `references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs:837`, `references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs:876`）。analytics 也把 pending steer request 与 accepted/rejected result 关联，并记录 `expected_turn_id`、accepted turn、rejection reason（`references/codex/codex-rs/analytics/src/reducer.rs:319`, `references/codex/codex-rs/analytics/src/reducer.rs:609`, `references/codex/codex-rs/analytics/src/reducer.rs:1377`; `references/codex/codex-rs/analytics/src/facts.rs:251`）。

Codex 对 `Op::UserInput` 的 server-side 分流仍是命令式的：有 active steerable turn 就 `steer_input`，否则把输入作为新 regular turn 启动；active turn not steerable 或 expected mismatch 直接 reject（`references/codex/codex-rs/core/src/session/handlers.rs:182`, `references/codex/codex-rs/core/src/session/handlers.rs:204`, `references/codex/codex-rs/core/src/session/handlers.rs:226`）。这类似 AgentDash 当前 `SendNext/Enqueue/Steer`，但 Codex 没有把这个分流本身持久化为 mailbox receipt。

Codex hook 与 mailbox 没有混成一个机制。`HookRuntime::inspect_pending_input` 只对 `TurnInput::UserInput` 执行 UserPromptSubmit，对 `ResponseItem` 和 `InterAgentCommunication` 不执行（`references/codex/codex-rs/core/src/hook_runtime.rs:499`）。UserPromptSubmit hook 可以 block、产生 additional context、或通过 plain stdout 注入 context；Stop hook block 会生成 continuation prompt/fragments，并在 same turn loop 中继续（`references/codex/codex-rs/hooks/src/events/user_prompt_submit.rs:34`, `references/codex/codex-rs/hooks/src/events/user_prompt_submit.rs:133`, `references/codex/codex-rs/hooks/src/events/stop.rs:62`, `references/codex/codex-rs/hooks/src/events/stop.rs:202`, `references/codex/codex-rs/core/src/session/turn.rs:322`）。这支持本任务设计里“hook control/context injection 不直接等于 mailbox delivery，hook-produced delivery/auto-resume 才进入 mailbox”的分层。

Codex interrupt 主要绑定当前 active turn 与 pending approval/request。v2 interrupt 校验 active turn id，terminal/no-running 时 reject；对于 approval request，interrupt 会 resolve pending approval（`references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs:1246`, `references/codex/codex-rs/app-server/tests/suite/v2/turn_interrupt.rs:210`）。TUI 还有一套 interrupt prompt 队列，用于 ExecApproval、ApplyPatchApproval、Elicitation、RequestPermissions、RequestUserInput 等 UI blocker，不与普通 pending user messages 混用（`references/codex/codex-rs/tui/src/interrupts.rs:16`, `references/codex/codex-rs/tui/src/interrupts.rs:89`）。

Codex UI 层会分开展示 queued user messages、pending steers、rejected steers。pending steers 在 agent turn running 时创建；interrupt 时如果存在 pending steers，会标记 interrupt 后提交（`references/codex/codex-rs/tui/src/chatwidget/input_queue.rs:14`, `references/codex/codex-rs/tui/src/chatwidget/input_queue.rs:21`, `references/codex/codex-rs/tui/src/chatwidget/input_submission.rs:322`, `references/codex/codex-rs/tui/src/chatwidget/interaction.rs:129`）。但 idle 后普通 queued input 只提交一个，slash/shell command 才可能连续 drain（`references/codex/codex-rs/tui/src/chatwidget/input_flow.rs:104`）。这更像前端投影和交互优化，不是 authoritative scheduler。

Codex resume 不是 pending mailbox resume。thread resume 协议恢复 thread subscription、initial turns page、active turn snapshot 与状态，running-thread resume 还会序列化 pending unload 后恢复视图（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs:329`, `references/codex/codex-rs/app-server/src/request_processors/thread_lifecycle.rs:529`）。Python SDK 的 `_GoalStreamCursor` 进一步把多个物理 continuation 合成一个 logical turn stream（`references/codex/sdk/python/src/openai_codex/_goal.py:37`, `references/codex/sdk/python/src/openai_codex/_goal.py:225`, `references/codex/sdk/python/src/openai_codex/_goal.py:250`）。这对 AgentRun projection 有启发，但不是 mailbox 存储模型。

### Naming and lifecycle boundary model

Codex 的公开层级是 `Thread -> Turn`。`Thread` 是持久 conversation/read model，包含 `id`、`session_id`、`status`、`turns` 等字段；`turns` 只在 `thread/resume`、`thread/rollback`、`thread/fork`、`thread/read(includeTurns)` 等响应里填充（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs:135`, `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs:175`）。`Turn` 是 thread 下的一次用户/客户端可见执行生命周期，含 `items`、`status`、`started_at`、`completed_at`、`duration_ms`（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs:185`）。`TurnStatus` 是 `Completed | Interrupted | Failed | InProgress`（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:28`）。

Codex 把启动、运行中追加输入、打断都命名为 turn 级命令：`TurnStartParams` 启动一个 turn，`TurnSteerParams` 要求 `expected_turn_id` 匹配当前 active turn，`TurnInterruptParams` 以 `thread_id + turn_id` 打断 active turn（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:66`, `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:165`, `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:199`）。Python SDK 也把 `Thread.run()` 描述为“run a complete turn”，`Thread.turn()` 返回可 steer/interrupt/stream 的 `TurnHandle`（`references/codex/sdk/python/src/openai_codex/api.py:554`, `references/codex/sdk/python/src/openai_codex/api.py:588`, `references/codex/sdk/python/src/openai_codex/api.py:719`）。

Codex core 内部不是用 `Run` 表达这层生命周期，而是 `Session` + `SessionTask` + `ActiveTurn`。`Session` 注释说一个 session 最多一个 running task，并可被 user input interrupt；它持有 `active_turn` 和 `input_queue`（`references/codex/codex-rs/core/src/session/session.rs:23`）。`SessionTask` 是“drives a Session turn”的异步任务抽象，具体 task 封装 regular chat、review、ghost snapshots 等 workflow（`references/codex/codex-rs/core/src/tasks/mod.rs:201`）。`TurnContext` 注释是“single turn of the thread”的上下文（`references/codex/codex-rs/core/src/session/turn_context.rs:89`）。

Codex lifecycle event 也围绕 turn：core protocol 有 `TurnStartedEvent`、`TurnCompleteEvent`、`TurnAbortedEvent`，abort reason 包含 `Interrupted`、`Replaced`、`ReviewEnded`、`BudgetLimited`（`references/codex/codex-rs/protocol/src/protocol.rs:1903`, `references/codex/codex-rs/protocol/src/protocol.rs:1921`, `references/codex/codex-rs/protocol/src/protocol.rs:3829`, `references/codex/codex-rs/protocol/src/protocol.rs:3844`）。app-server v2 也有 `TurnStartedNotification` 和 `TurnCompletedNotification`，另有 turn-level diff/plan notifications（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:368`, `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:385`, `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:393`, `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:404`）。

Resume/interrupt 边界在 Codex 命名里分得很清楚：resume 是 `thread/resume`，用于加载、重连或 rejoin running thread，并可返回 active turn snapshot；interrupt 是 `turn/interrupt`，必须匹配 active turn，否则 rejected（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs:315`, `references/codex/codex-rs/app-server/src/request_processors/thread_lifecycle.rs:540`, `references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs:1246`, `references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs:1263`）。如果 resume/read 时 thread 不再 active，Codex 会把 stale `InProgress` turn 标成 `Interrupted`，说明恢复视图不是继续执行本身（`references/codex/codex-rs/app-server/src/request_processors/thread_lifecycle.rs:778`）。

Codex 明确避免对同一个 active turn 发出重复 lifecycle。`UserShellCommandMode` 区分 `StandaloneTurn` 和 `ActiveTurnAuxiliary`：前者作为独立 turn lifecycle 发 `TurnStarted/TurnComplete`，后者运行在现有 active turn 内，不能发第二组 lifecycle event，否则会 confuse clients（`references/codex/codex-rs/core/src/tasks/user_shell.rs:50`, `references/codex/codex-rs/core/src/tasks/user_shell.rs:112`）。这点对 AgentDash 很关键：内部小轮、辅助工作、mailbox 消费都不应伪装成新的大生命周期。

Codex 也有 logical-vs-physical turn 的投影层。Python `_GoalOperationState` 同时维护 `logical_turn_id` 和 `current_turn_id`，`_GoalStreamCursor` 把多个 physical goal events 消费为一个 ordered logical turn stream（`references/codex/sdk/python/src/openai_codex/_goal.py:37`, `references/codex/sdk/python/src/openai_codex/_goal.py:41`, `references/codex/sdk/python/src/openai_codex/_goal.py:42`, `references/codex/sdk/python/src/openai_codex/_goal.py:250`）。测试断言三个 runtime request/continuation 最终只暴露 `["turn/started", "turn/completed"]` 一组 lifecycle，所有 routed id 都是 logical turn id（`references/codex/sdk/python/tests/test_app_server_goal_operations.py:18`, `references/codex/sdk/python/tests/test_app_server_goal_operations.py:83`）。

`Run`、`Cycle`、`Invocation`、`Step` 在 Codex 里都不是这层主生命周期名。`Run` 主要出现在 Python `RunInput` / convenience `run()`，以及 hook runtime 的 `HookRunStatus` / `HookRunSummary`（`references/codex/sdk/python/src/openai_codex/_inputs.py:47`, `references/codex/codex-rs/app-server-protocol/src/protocol/v2/hook.rs:69`, `references/codex/codex-rs/app-server-protocol/src/protocol/v2/hook.rs:90`）。`Step` 出现在 turn plan step（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:414`）。检索未发现 `SessionRun` 或 `Invocation` 作为 Codex turn/session lifecycle 的核心命名；`Cycle` 也不是该领域层对象。

### Naming recommendation for AgentDash

AgentDash 的“大轮”应避免叫裸 `Turn`。Codex 可以把用户可见执行生命周期叫 `Turn`，是因为它的公开模型没有同时把 agent-loop 内部小轮作为另一个同名业务边界暴露出来；AgentDash 当前已经有 `AgentEvent::TurnStart/TurnEnd` 表示内部小轮，又有 `TurnExecution start_prompt -> Terminal` 表示大生命周期。继续把大轮叫 `Turn` 或 `TurnExecution` 会保留本任务正在切除的歧义。

Superseded: the early recommendation to use `AgentRunCycle` was based on an over-weighted prompt hint. The final task direction is to align with Codex `Thread/Turn`.

Final naming direction:

- `AgentRunThread` names the AgentRun workspace-side conversation/execution container.
- `AgentRunTurn` names the user-visible `start_prompt -> stop/terminal` execution lifecycle, matching Codex `Turn`.
- `AgentLoopTurn` names the current agent loop `AgentEvent::TurnStart/TurnEnd` boundary when referenced from AgentRun/mailbox code, so PiAgent internal turns remain explicit without competing with `AgentRunTurn`.
- Codex-style active turn preconditions should stay turn-named: `expected_active_agent_run_turn_id`, accepted AgentRun turn id, and accepted protocol turn id.

`InternalTurn` 或 `AgentLoopTurn` 应保留给小轮边界：`AgentEvent::TurnStart/TurnEnd`、steering queue poll、`InternalTurnBoundary`。如果代码层不能马上改掉 `TurnEnd` 事件名，新的 mailbox/domain 文档和类型也应始终写 `internal_turn` / `agent_loop_turn`，不要让 `Turn` 独立出现。

不建议把大轮改叫 `Run`。AgentDash 顶层已经有 `AgentRun`，`Run` 再作为子生命周期会变成 `AgentRunRun` 语义；Codex 里 `Run` 也不是 thread/turn lifecycle，而是 SDK convenience input 和 hook run 实例。`SessionRun` 也不建议，Codex 的 `Session` 是 live runtime context，AgentDash spec 也把 Session 留给 RuntimeSession/connector 边界。

`Invocation` 可作为更窄的实现词，表示一次 launch/connector 调用或 scheduler delivery attempt，但不适合作为用户可见大生命周期。因为 mailbox 在同一个大生命周期里可能多次 steer、continue、reopen delivery phase，`Invocation` 容易让人误以为每条 message 或每次 delivery 都产生一个新的生命周期。

`Step` 不适合大轮命名。Codex 已把 `Step` 用在 plan step，AgentDash 也容易把它和 workflow step、tool step、scheduler step 混淆。它只适合表达 plan/checklist/scheduler 的细粒度动作。

如果需要同时表达投影层和物理执行层，可借鉴 Codex SDK 的 `logical_turn_id/current_turn_id`，并保留 turn 语义：`logical_agent_run_turn_id` / `active_agent_run_turn_id` / `physical_execution_id`。这能支持“多个内部 continuation 对用户仍是一次 AgentRunTurn”的 read model，同时通过 `AgentLoopTurn` 前缀明确内部 turn 层级。

### Suggestions for AgentRun mailbox/barrier design

1. Adopt a durable version of Codex's dual-buffer model: mailbox is session/run-scoped durable intake; active execution has a claim/phase gate. AgentRun should keep `barrier` and `drain_mode`, but add an explicit runtime delivery phase similar to `CurrentTurn | NextTurn` so `InternalTurnBoundary` means “current internal cycle can accept this envelope now,” not merely “runtime emitted a TurnEnd event.”

2. Model “wake a turn” separately from “deliver into current turn.” Codex's `trigger_turn` is useful as the minimum distinction, but AgentRun already needs richer fields: `delivery`, `barrier`, `drain_mode`, `origin`, and `source_dedup_key`. A hook/system/follow-up envelope can wake idle work without being eligible to append into a closing active turn.

3. Preserve model-continuation priority in the barrier state machine. Codex explicitly delays pending/steered input around auto-compact and required model/tool continuations. AgentRun should avoid a naive “any internal turn end drains all InternalTurnBoundary envelopes” rule; drain should wait for a safe loop boundary after mandatory continuation/compaction work has been recorded.

4. Add active-turn preconditions and result telemetry to command receipts. Codex's `expected_turn_id`, accepted `turn_id`, and rejection reasons are a strong pattern; AgentRun mailbox commands should translate this to `expected_active_agent_run_turn_id`, observed/accepted AgentRun turn id, accepted protocol turn id, accepted envelope ids, and typed rejection/deferral reasons. A non-steerable or mismatched active turn should be a command result, not an AgentRun failure.

5. Keep hook control paths separate from mailbox delivery. UserPromptSubmit-style block/rewrite/context injection should stay in hook runtime. Hook outputs that are actually user/system-visible follow-up messages, auto-resume prompts, or delivery messages should become mailbox envelopes with stable dedup keys and normal barrier semantics.

6. Let frontend show categories without owning scheduling authority. Codex TUI categories are useful UX: queued ordinary messages, pending steers, rejected steers, and interrupt blockers are visually different. AgentRun can project similar mailbox statuses, but the server-side durable mailbox should remain the source of truth for ordering, barrier eligibility, and drain.

7. Coalesce internal physical continuations in the read model. The Python SDK goal stream suggests a practical projection rule: multiple internal continuations can still appear as one user-visible AgentRunTurn until a logical terminal boundary. This fits the task's “AgentLoopTurn vs AgentRunTurn” split while keeping protocol vocabulary aligned with Codex `Turn`.

8. Standardize lifecycle naming around Codex-compatible `AgentRunThread` / `AgentRunTurn`. Use `AgentLoopTurn` for agent-loop boundaries when referenced from AgentRun mailbox code, and reserve `Invocation` for delivery/connector attempts. Avoid `Run` because `AgentRun` already owns that word at the workspace level.

### Not directly copyable

- Codex pending mailbox is mostly in-memory (`Mutex<VecDeque>` plus watch notification), while AgentRun requires durable DB rows, claims, idempotency, recovery, and command receipts.
- Codex `trigger_turn: bool` is too coarse for AgentRun. It cannot express `ImmediateIfIdle` vs `InternalTurnBoundary` vs `AgentTurnTerminal` vs `ManualResume`, nor one-vs-all drain.
- Codex server still uses route/runtime-local steer-or-new-turn branching for `Op::UserInput`; AgentRun is intentionally converging away from route-local `SendNext/Enqueue/Steer` into a unified mailbox command model.
- Codex stores delivered inter-agent mail into rollout transcript, but that is not the same as a durable pending mailbox. AgentRun should not rely on transcript/log projection as the scheduling source.
- Codex Stop hook continuation is loop-local prompt continuation. AgentRun hook delivery/auto-resume should be durable mailbox/outbox effects, especially because the task already requires crash recovery and dedup.
- Codex resume is thread/view rehydration, not mailbox resume. AgentRun “resume” should remain a mailbox/barrier command concept, while UI/session rehydration stays in the read model.
- Codex TUI pending steer queues are client-side interaction state. They are useful for projection and UX but should not be copied as backend authority.
- Codex's public `Turn` naming should not be copied directly into AgentDash's big lifecycle unless the internal `TurnStart/TurnEnd` vocabulary is also renamed. Otherwise the same confusion documented in this task remains in the new mailbox model.
- Codex's `Run` vocabulary should not be used as a replacement for AgentDash big turn: in Codex it is not the thread/turn lifecycle name, and in AgentDash it collides with the existing top-level `AgentRun`.

### External references

No external web/docs were used. This research is based on the local `references/codex` snapshot and current Trellis task/spec documents only.

## Caveats / Not Found

- I did not find a durable Codex mailbox table or receipt model equivalent to the planned AgentRun mailbox. The closest durable artifacts are rollout/transcript records and app-server analytics around turn steer results.
- I did not find Codex hooks producing first-class mailbox envelopes. Hooks produce block/stop decisions, additional context, continuation prompts, and hook events; delivery-message convergence appears to be an AgentDash-specific design need.
- I did not find a direct “barrier” enum in Codex. The closest concepts are `MailboxDeliveryPhase`, `trigger_turn`, active-turn steerability, and `can_drain_pending_input`.
- I did not find `SessionRun`, `Invocation`, or `Cycle` used as Codex's core session/turn lifecycle object names. Those would be AgentDash-local terminology choices, not terms borrowed from Codex.
- I did not inspect code outside `references/codex`, `.trellis/spec/`, and the current task documents, per scope.

## Main-session correction

The research prompt was biased by an early `AgentRunCycle` naming proposal. Final planning should not treat that as the preferred direction.

User decision after review:

- AgentDash control-plane protocol should align with Codex app-server protocol.
- AgentRun workspace concepts should use a `Thread/Turn` model.
- The large user-visible lifecycle should be `AgentRunTurn`, matching Codex `Turn` semantics.
- The current agent loop `AgentEvent::TurnStart/TurnEnd` should be labeled at the AgentRun/mailbox boundary as `AgentLoopTurn`, so internal PiAgent turns do not compete with the public `AgentRunTurn` concept.
- `Cycle` should not be introduced as the primary lifecycle name unless there is a future protocol-level reason.
