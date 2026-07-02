# 实现 Agent 并行等待与 mailbox 回传能力

## Goal

在 AgentDashboard 自有 AgentRun、companion、exec、lifecycle gate、command receipt、mailbox 和 frontend workspace 体系内，实现 Agent 并行工作、挂起等待、事件唤醒、结果回传的闭合能力。

本任务参考 Codex 的闭合能力集；Codex 不作为运行时依赖或项目事实源。Codex 只作为能力模型参考：spawn/message/wait/close 解耦，wait 等待 activity/mailbox 变化，结果通过 mailbox/notification 回到等待方。

## Confirmed Facts

- AgentRun mailbox 已经是 durable message intake、调度队列和恢复投影，包含 origin/source identity、delivery、barrier、drain mode、status、claim lease、accepted refs、dedup 和 command receipt linkage。
- Companion/subagent 已有 `companion_request`、`companion_respond`、LifecycleGate、child dispatch、parent mailbox result delivery、human response delivery。
- Hook runtime 已有 before/after turn、before stop、subagent dispatch、companion result 等触发点；terminal completed fallback 可 schedule AgentRun turn boundary。
- 当前缺少 first-class “Agent 正在等待某个 companion/exec/subagent/human event” 的 durable wait projection。
- `wait=true` companion 主要是 tool 内轮询 gate；这会占用当前 tool/turn，不等价于 durable suspend/resume。
- 当前没有通用 exec/subagent event resolved -> mailbox wake/result -> waiting Agent resume 的 adapter。
- Codex v2 wait 更适合作为参考：wait 返回 activity 摘要和 timeout，不搬运大结果；结果内容通过 mailbox/thread item 查询。
- 实现不得新增或依赖旧 Session 形态对外端点。RuntimeSession 可以作为 delivery ref，但不能重新成为 workspace command owner。

## Requirements

1. 定义 wait owner：companion/subagent/human/exec 等待的 durable source of truth 应落在 LifecycleGate 或同级 lifecycle wait record；mailbox 只承载 wake/result envelope。
2. 增加 AgentRun workspace waiting projection，让前端能看到当前 Agent 正等待哪个 companion/exec/human/subagent 事件。
3. 建立通用 wake adapter：事件完成后构造 source identity、source dedup、payload preview，写入 AgentRun mailbox，并触发 scheduler/notification。
4. companion wait 必须从 tool 内长期轮询逐步收束到 durable wait + mailbox resume。短期实现必须至少为现有 wait 补齐 timeout、projection 和可恢复 result delivery。
5. subagent spawn/send/result/close 必须以项目自有 companion/lifecycle/mailbox 事实表达；不得引入 Codex Thread/AgentPath 作为 domain identity。
6. exec completion、failure、cancel 应可映射为 wait resolution 和 mailbox result envelope，供 waiting Agent 或 workspace UI 消费。
7. wait 行为应等待 durable mailbox state、gate resolution 或 runtime event activity；返回摘要、timeout 和 message refs，不直接传输大段结果。
8. frontend workspace 必须展示 wait items、mailbox result、companion events，并在 mailbox/gate state changed 后刷新。
9. 新增或调整跨层 DTO 后必须重新生成 TypeScript contracts，并补 drift/URL tests。

## Acceptance Criteria

- [ ] PRD/design/implementation 明确 wait owner、wake envelope、projection、scheduler trigger 和 frontend refresh 边界。
- [ ] AgentRun workspace projection 能返回 open wait items，至少覆盖 companion/subagent/human 等待；exec wait 可作为同一模型扩展。
- [ ] companion/subagent result 完成后通过 mailbox wake envelope 进入 parent AgentRun，并触发 `mailbox_state_changed` 或等价通知。
- [ ] wait 工具/能力对已有 pending mailbox activity 可立即返回，对未来 activity 可挂起等待，对 timeout 返回明确 timeout。
- [ ] result 内容保留在 mailbox/message/projection 中；wait 返回摘要和引用，不搬运大结果。
- [ ] scheduler tests 覆盖 idle launch、running boundary drain、timeout、duplicate result dedup、delivery_result_unknown recovery。
- [ ] frontend tests 覆盖 wait item 展示、mailbox result 刷新、companion source label 和 no stale UI。
- [ ] 代码搜索确认新增实现不暴露旧 Session 形态 endpoint，不引入 Codex runtime dependency。

## Out Of Scope

- 终端显示/跳转/PowerShell 对象输出修复，由 `.trellis/tasks/07-02-terminal-output-navigation-repair` 承接。
- 全项目旧 Session API 总清理。
- 复制 Codex input queue、Codex Thread/AgentPath identity 或 Codex app-server runtime。

## Research

- `.trellis/tasks/07-02-agent-parallel-wait-mailbox-implementation/research/current-mailbox-companion-parallel-capability.md`
- `.trellis/tasks/07-02-agent-parallel-wait-mailbox-implementation/research/codex-reference-closure.md`
