# 收敛 AgentRun 会话状态与消息队列标注行为

## Goal

把 AgentRun 中“发送消息推进会话”的模型从多条特殊命令路径收敛为一个统一的 AgentRun Mailbox。

本质上，用户消息、steer 消息、hook/system steering、普通 pending message 都是投递给同一个 AgentRun delivery runtime 的 message envelope。它们的差异不应体现为多套状态机，而应体现为：

- 谁产生了消息：user / system / hook / companion / workflow。
- 消息何时能被消费：当前 AgentRunTurn active、AgentLoopTurn 边界、AgentRunTurn stop/terminal 边界、或 runtime idle。
- 消息如何被消费：steer 当前 turn、继续当前 turn、launch/resume 新 turn、丢弃/删除、或暂停等待用户恢复。

目标状态是：前端只向 AgentRun workspace 投递 message command；后端把 message 放入 mailbox，并由 mailbox scheduler 根据当前 runtime state 和 message policy 决定立即消费、排队、暂停或恢复。

## Confirmed Facts

- 当前前端用户入口集中在 `AgentRunWorkspacePage.tsx` 与 `SessionChatView.tsx`：draft 首轮、composer submit、cancel、promote pending、delete pending、resume pending queue。
- 当前后端 `submit_agent_run_composer_input` 根据 `SessionExecutionState` 把 composer submit 分类为 `SendNext`、`Enqueue` 或 `Steer`。
- `SendNext` 走 `AgentRunMessageService::dispatch_user_message`，有 durable delivery command receipt，并调用 `SessionLaunchService`。
- `Enqueue` 当前写入进程内 `PendingQueueService(HashMap)`，后端重启会丢失 pending message 和 pause state。
- `Steer` 当前通过 `AgentRunSteeringService::steer` 注入 connector，并写 `user_input_submitted` runtime event，但没有同级 durable command receipt。
- `promote_pending` 当前把 pending message 取出后作为 steer 注入 active turn；成功响应是裸 JSON，前端服务层丢弃返回值。
- `turn` 在当前项目里有两层含义：
  - AgentLoopTurn：`agentdash-agent/src/agent_loop.rs` 中的 `AgentEvent::TurnStart/TurnEnd`，一次 assistant response/tool cycle 结束不代表 AgentDash session 停止。
  - AgentRunTurn：当前代码里对应 `TurnExecution`，生命周期是一次 `start_prompt -> terminal`，由 `TurnState` 管理，收到 `TurnEvent::Terminal` 后才 `clear_active_turn`。
- `SessionExecutionState` 优先读取进程内 `TurnState`；如果不 running，才从 `SessionMeta.last_delivery_status/last_turn_id` 派生 completed/failed/interrupted/idle。
- 当前 completed terminal callback 会 drain pending queue 队首；failed/interrupted 会 pause queue；resume pending queue 可恢复 drain。
- 现有 `LaunchSource` 已区分 `LifecycleAgentUserMessage`、`HookAutoResume`、`CompanionParentResume`、`WorkflowOrchestrator`、`RoutineExecutor` 等来源。
- Pi Agent 的 steer 入口当前只是把消息 push 到 `steering_queue`；agent loop 在内部 `AgentEvent::TurnEnd` 后 `poll_steering`，并按 `QueueMode::All` 或 `QueueMode::OneAtATime` 出队。
- Hook 当前有多类 delivery-adjacent 输出：`UserPromptSubmit` 可阻止/改写输入并注入上下文，`AfterTurn` / `BeforeStop` 可产出 steering/follow_up；目标模型里两者都归一为 steering，`follow_up` 只是 stop 边界上 steering 继续当前 loop 的效果。`HookAutoResume` 通过 terminal effect outbox 触发后续 launch。
- 产品方向：用户 message 不需要作为长期聊天内容副本额外持久化；系统 pending message 应 durable；用户 message 可以进入同一个 mailbox，但持久化重点是 envelope、状态、来源、消费结果和必要恢复 payload。

## Core Concepts

| Concept | 说明 |
| --- | --- |
| AgentRun Mailbox | AgentRun workspace 的统一 message intake 和待消费队列。 |
| Message Envelope | 一条可被消费的消息，包含 origin、source、payload、消费策略、状态、receipt 关联和 projection 摘要。 |
| Consumption Barrier | 何时允许消费：`immediate_if_idle`、`agent_loop_turn_boundary`、`agent_run_turn_boundary`、`manual_resume`。 |
| Delivery Adapter | 如何消费：`launch_or_continue_turn`、`steer_active_turn`、`resume_launch_source`、`delete`。 |
| Drain Mode | 一个 barrier 满足时消费几条：`one` 或 `all`。 |
| Command Receipt | 用户可见 command 的幂等边界；不直接表达业务调度模型。 |
| Scheduler | 根据 runtime state、barrier、priority 和 queue order 原子 claim 下一条可消费消息。 |

## Message Policy Draft

| Message type | Origin | Barrier | Delivery | Drain mode |
| --- | --- | --- | --- | --- |
| 用户新消息，runtime idle/completed/failed/interrupted | user | `immediate_if_idle` | `launch_or_continue_turn` | `one` |
| 用户新消息，runtime running 且支持 steer | user | `agent_loop_turn_boundary` | `steer_active_turn` | `all` |
| 用户新消息，runtime running 且不采用 steer | user | `agent_run_turn_boundary` | `launch_or_continue_turn` | `one` |
| hook/system AgentLoopTurn 消息 | system/hook | `agent_loop_turn_boundary` | `steer_active_turn` | `all` |
| before_stop steering / stop gate retry | system/hook | `agent_loop_turn_boundary` 或 `agent_run_turn_boundary` | `steer_active_turn`，stop 时继续当前 loop | `all` 或 `one` |
| promote pending | user command | 把指定 envelope 改为 `agent_loop_turn_boundary` | `steer_active_turn` | 指定消息优先消费 |
| delete pending | user command | 无 | `delete` | 标记指定 1 条 |

## Requirements

- 建立 AgentRun Mailbox 作为统一消息队列与状态投影，替代“send_next/enqueue/steer/promote/resume 各自解释消息状态”的模型。
- 控制面事实源必须落在 backend envelope、domain/application crates 和 durable repository 中；Codex app-server protocol 是优先复用和整体对齐的协议基线，不是完整天花板。
- AgentRun workspace 层使用 `Thread/Turn` 语义；delivery adapter 优先映射到 `thread` read/resume、`turn/start`、`turn/steer`、`turn/interrupt` 等 Codex 原语。
- Codex protocol 的有限偏移是允许的，但必须是 backend envelope 明确表达的超集语义，有 typed adapter、状态投影和测试覆盖，不能藏在 route-local branch 或 connector 私有副作用里。
- 这是模型对齐重构，现有命名和事实结构都可以批量修改；`TurnExecution`、pending DTO/API、route-local command kind、in-memory queue 等旧事实都不是兼容性边界。
- 所有用户可见 message command 都必须有 `client_command_id` 幂等 receipt；重复提交不应重复写入 mailbox、重复 steer、重复 launch 或表现为不可解释 404。
- Mailbox envelope 必须记录 origin、source、barrier、delivery、status、priority/order、preview、payload retention policy、accepted refs/result。
- Scheduler 必须以后端当前 runtime state 为准决定消费：
  - idle/completed/failed/interrupted 且 AgentRun 可继续时，新消息可以 resume/launch 到新 AgentRunTurn。
  - running active 时，符合 steer policy 的消息进入 `agent_loop_turn_boundary`，在 agent loop 内部 `TurnEnd` 后批量注入下一 AgentLoopTurn。
  - AgentRunTurn stop/terminal 边界默认只消费下一条普通 pending user message；若仍在 `BeforeStop` active context 中消费，则作为 steering 继续当前 loop，terminal callback 作为 fallback。
  - failed/interrupted 后暂停需要人工确认的 queue，但不阻止后续新用户消息以新 command resume 到下一轮。
- Scheduler 必须把 `barrier` 与 `drain_mode` 分开建模：AgentRunTurn 边界的普通用户消息 drain mode 是 `one`；AgentLoopTurn 的 steer/hook 消息 drain mode 通常是 `all`，与现有 `QueueMode::All` 语义对齐。
- Hook 收束范围必须明确：hook 的策略判断、上下文注入、工具审批仍属于 hook runtime；hook 产出的 delivery message，包括 after-turn steering、before-stop steering、anchored hook auto-resume，必须进入 mailbox envelope，由 barrier/drain mode 调度。现有 hook `follow_up` 语义归并到 stop 边界 steering 的 continue effect。
- Mailbox 状态管理必须支持恢复：message claim 要有 attempt/claim token/lease 或等价机制；进程重启后未完成的 `Consuming` message 要能恢复到可重试或 blocked 状态；hook terminal effect replay 不能重复创建 system-origin envelope。
- 系统 pending message 必须 durable；user-origin payload 可以 queued 时临时持久以支持恢复，消费成功后按 retention policy 清理，避免形成第二份长期聊天记录。
- 前端继续只消费后端 projection：message 是否 queued、steered、dispatched、paused、blocked、failed 由 workspace/mailbox projection 表达。
- 保留现有 runtime connector 边界：steer 仍通过 connector steer，launch/resume 仍通过 `SessionLaunchService`，Mailbox 不替代 session runtime command outbox。

## Acceptance Criteria

- [ ] `current-state.md` 明确列出当前 route-local 分类、进程内 pending queue、terminal drainer、synthetic receipt、pending DTO/API、internal-turn trigger、hook delivery message 和 hook auto-resume 的旧线条。
- [ ] `design.md` 定义 AgentRun Mailbox、Message Envelope、Consumption Barrier、Delivery Adapter、Scheduler 和 Command Receipt 的边界。
- [ ] `design.md` 明确区分 AgentRunThread、AgentRunTurn 与 AgentLoopTurn，并说明每类消息在哪个 barrier 消费。
- [ ] `design.md` 明确 AgentRun backend envelope 是 Codex app-server protocol 的控制面超集；所有有限偏移都有 schema/domain/adapter/test 对应落点。
- [ ] `implement.md` 将重构拆成 storage/domain、scheduler、API command、terminal integration、frontend projection、test/check 切片。
- [ ] composer submit 不再暴露为三套分裂的业务路径；后端写入或消费 mailbox 后返回统一 command receipt + envelope/accepted refs。
- [ ] running steer、running queue、idle resume/new turn 都由同一个 mailbox scheduler 解释，不由前端判断。
- [ ] agent loop 内部 `TurnEnd` 后会触发 mailbox scheduler，消费所有符合 `agent_loop_turn_boundary + drain_mode=all` 的 steer/hook 消息。
- [ ] hook 产出的 delivery message 不再绕过 mailbox；direct hook runtime 只保留策略判断、上下文注入、工具审批和 trace 记录。
- [ ] AgentRunTurn stop/terminal 边界最多自动消费一条 `agent_run_turn_boundary + drain_mode=one` 的普通用户消息；若在 `BeforeStop` 命中则以 steering 继续当前 loop；failed/interrupted 会标注暂停，但新用户消息仍可创建新 envelope 并触发 resume policy。
- [ ] promote/delete/resume 重试基于 command receipt 和 envelope 状态返回稳定结果。
- [ ] mailbox status 支持 `Queued/ReadyToConsume/Consuming/Dispatched/Steered/Paused/Blocked/Failed/Deleted` 的恢复语义，进程重启后不会丢失 claim 中的 envelope。
- [ ] hook auto-resume terminal effect replay 通过 source/effect id 幂等写入 mailbox，不会重复创建 system envelope。
- [ ] 后端重启后，mailbox envelope、pause state、system pending message 和未消费 user pending message 可恢复。
- [ ] 前端 pending/message UI 能展示 queued、ready_to_consume、consuming、dispatched、steered、paused、blocked、failed、deleted 等必要状态。
- [ ] `current-state.md` 中的 cut-line grep commands 不再命中旧模型的生产权威路径。
- [ ] 批量重命名完成：public contract、backend DTO/domain、frontend service/projection 不再以 pending queue / route-local command kind / ambiguous turn vocabulary 作为主模型。
- [ ] 测试覆盖：idle 新消息 launch、running steer、running no-steer queue、AgentRunTurn 边界 consume one、BeforeStop steering continue、failed pause、新消息恢复下一 turn、promote 指定消息、delete 幂等、system hook steering durable。

## Out of Scope

- 不重新设计 AgentRun workspace 页面布局。
- 不改变 connector 内部 steering / prompt 能力。
- 不把 `session_runtime_commands` 改造成 mailbox；它仍是 frame/runtime capability delivery outbox。
- 不把 user-origin payload 保存为长期聊天 transcript；真正被接受的用户输入仍以现有 session event 为准。
- 不做旧内存 pending queue 的兼容迁移。

## Decisions

- 采用 mailbox/barrier 模型替代 pending-channel-first 模型。
- 目标命名对齐 Codex `Thread/Turn` 模型：`AgentRunThread` 表达 AgentRun workspace 侧会话容器，`AgentRunTurn` 表达一次 `start_prompt -> stop/terminal` 的用户可见执行；现有 agent loop 内部 `TurnStart/TurnEnd` 在目标模型中标注为 `AgentLoopTurn`，通过前缀区分内部 loop turn 和用户可见 turn。
- Mailbox/backend envelope 是 AgentDash 控制面事实源，也是 Codex protocol 的 AgentRun 超集；所有运行时控制应优先复用 Codex-compatible thread/turn 操作。有限偏移必须显式落在 envelope schema、domain enum、adapter mapping 和测试中。
- 将“send_next / enqueue / steer”视为同一消息在不同 runtime state 下被 scheduler 选择的消费结果，而不是三种根业务模型。
- 系统 pending 与用户 pending 共享 mailbox envelope；差异由 origin/source/barrier/delivery/retention policy 表达。
- Hook delivery message 也进入同一 mailbox；非 delivery hook 行为不进入 mailbox，避免把策略评估和消息调度混成一层。
- AgentRunTurn 以 `TurnExecution start_prompt -> TurnEvent::Terminal` 为准；agent loop 内部 `TurnStart/TurnEnd` 是 AgentLoopTurn 消费 barrier，`BeforeStop` 是 AgentRunTurn 即将结束但尚可继续当前 loop 的边界，不能替代 workspace execution state。
- Steer 的 runtime 原语应对齐 Codex `turn/steer(expected_turn_id)`；mailbox 里的 `agent_loop_turn_boundary` 只是服务端调度/消费 phase，不是公开 protocol 方法。
- `drain_mode` 是 policy 的一部分：AgentRunTurn 边界用户 pending 默认 `one`，AgentLoopTurn steer/hook 默认 `all`。
- 批量机械重命名是本任务推荐路径；与目标模型冲突的旧类型名、DTO 名、endpoint 名和 projection 名应直接迁移，不保留兼容 alias。
