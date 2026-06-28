# Routine 单会话与 Companion Mailbox 收束设计

## Goal

收束当前短期最应该处理的 AgentRun 入站主线：Routine 单会话模式与 Companion 协作通信。目标是让 Routine 对已有会话的后续触发，以及 Companion sub / parent / human 交互中“需要让某个 AgentRun 继续处理的一段输入”，都进入 AgentRun Mailbox 的 durable envelope 与 scheduler。

本任务是架构规划任务，不直接实现代码。规划完成后在同一任务下以工作项推进、追踪和验证。

## Background

当前代码里已经存在两个可收束的基础：

- AgentRun Mailbox 已经承担 user composer、ProjectAgent initial message、Canvas input、hook steering / auto-resume 的 durable intake 与 turn boundary scheduler。
- Routine 与 Companion 已经有各自的 durable facts，但投递入口仍绕过 mailbox 主线。

已确认的现状：

- RoutineExecutor 当前路径是 `trigger -> RoutineExecution -> LifecycleDispatchService`，`DispatchStrategy::Reuse` 会复用已有 run/agent，但 prompt 仍作为 Routine dispatch 的内部输入，而不是进入目标 AgentRun mailbox。
- `MailboxMessageSource` 已预留 `routine_executor` 和 `workflow_orchestrator`，但 RoutineExecutor 尚未通过 `AgentRunMailboxService` materialize message。
- Companion `target=sub` 当前会创建 child AgentRun 后直接调用 `launch_command_with_outcome`，child 首条任务未进入 child mailbox。
- `companion_respond` 会尝试命中 parent-owned gate、hook pending action、child-owned gate 三类副作用；这些命中不是互斥路径。
- `target=parent` 当前已创建 parent-owned `LifecycleGate` 并向 parent runtime 发送 notification；resolve 后仍以 notification 表达。
- `target=human` 当前创建 gate 并向当前 runtime 发送 human request notification；用户 respond 后写 gate，再注入 runtime notification。
- `target=platform` 当前只有未启用 broker 的错误路径；它不是本轮 mailbox 投递主线，但需要保留清晰边界，避免后续绕回 ad hoc notification。
- Companion notification delivery 失败通常只记录 warn，不具备 mailbox 的 claim/recovery/paused/manual resume 语义。
- Mailbox migration 的 source check 与代码 enum 已有 drift：domain/API 已包含 `canvas_action`，migration `0013_agent_run_mailbox.sql` 尚未包含。

## Requirements

- R1: 明确 Routine 单会话模式的目标语义：外部/定时触发命中已有 AgentRun 时，应以 mailbox message 表达后续输入，由 mailbox 决定 idle launch、running boundary queue、paused/manual resume。
- R2: 明确 Routine Fresh / PerEntity / Reuse 三种 dispatch strategy 哪些仍保留 lifecycle creation，哪些在已有 run/agent 下切换为 mailbox intake。
- R3: 明确 RoutineExecution 与 mailbox message 的关联关系，保留 RoutineExecution 作为触发、模板、entity memory、执行结果的事实源，同时将面向 Agent 的后续输入落到 mailbox。
- R4: Companion 作为一等收束目标覆盖完整交互面：child initial task、child result to parent、child parent request、parent response to child、human request、human response 都需要有 mailbox delivery 事实；LifecycleGate 保留为等待、审阅、采纳与 correlation 事实。
- R5: 明确 LifecycleGate 在收束后的角色：负责等待、审阅、采纳、关联 parent/child/human request，而不负责替代消息投递事实。
- R6: 设计 Companion mailbox envelope 的来源、dedup、correlation、payload retention、preview 和 frontend projection。
- R7: Companion 与 Routine 入站策略复用现有 mailbox delivery/barrier/drain_mode，作为同一 workspace mailbox/status 面的可观察消息。
- R8: Routine 后续触发和 Companion 回流在 AgentRun workspace 的 mailbox/status 区域可观察、可暂停、可恢复、可删除或可重排。
- R9: Host Integration 自定义信道系统由长期 draft 承载；本任务仅为未来信道打好 AgentRun mailbox 入站边界。
- R10: 识别当前 schema drift 与必要 migration：mailbox source check、domain enum、API mapper、generated TS 必须一致。

## Acceptance Criteria

- [ ] `design.md` 给出 Routine 单会话模式进入 mailbox 的推荐数据流。
- [ ] `design.md` 给出 Companion sub / parent / human 全交互面进入 mailbox 的推荐数据流。
- [ ] `design.md` 明确 RoutineExecution、LifecycleGate、AgentRunMailboxMessage 三者的职责边界。
- [ ] `design.md` 覆盖失败恢复、重复触发、running turn boundary、paused mailbox、manual resume 的行为。
- [ ] `design.md` 明确数据库 / domain / DTO / frontend projection 需要调整的方向。
- [ ] `implement.md` 以同一任务下的工作项列出可执行推进计划、依赖、验收和验证命令。
- [ ] `work-items/` 拆出当前任务内工作项文件，并在 `subagent-dispatch.md` 明确 sub-agent 派发规范、并行可行性和最优执行 wave。
- [ ] `implement.jsonl` 与 `check.jsonl` 使用真实 spec / research 清单，支持后续 sub-agent 执行和检查。

## Out Of Scope

长期 Agent 自定义信道、群聊信道、通用 channel broker 和 Host Integration 事件源模型由 `.trellis/tasks/06-28-agent-custom-channel-draft` 承载。本任务聚焦当前 Routine / Companion 的 AgentRun mailbox 收束。

Platform capability grant broker 尚未形成可投递业务事实。本任务只明确 `target=platform` 后续必须接入 broker / mailbox 边界，当前不把未实现 broker 当成可交付消息链路。

## Open Questions

无阻塞问题。当前决策是 Companion 交互面一并收束，工作项在同一任务内追踪。
