# Implementation Plan

后续执行不创建 Trellis 子任务，统一在当前 task 下按工作项文件推进、记录进度和验证结果。

## Work Item Files

工作项文件位于 [work-items/](./work-items/)：

- [W0: Source And Schema Baseline](./work-items/W0-source-schema-baseline.md)
- [W1: Mailbox Intake Command Shape](./work-items/W1-mailbox-intake-command-shape.md)
- [W2: Routine Reuse Into Mailbox](./work-items/W2-routine-reuse-mailbox.md)
- [W3: Companion Sub Dispatch Into Child Mailbox](./work-items/W3-companion-sub-dispatch.md)
- [W4: Companion Child Result To Parent Mailbox](./work-items/W4-companion-child-result.md)
- [W5: Companion Parent Request And Response](./work-items/W5-companion-parent-request-response.md)
- [W6: Companion Human Request And Response](./work-items/W6-companion-human-request-response.md)
- [W7: Platform Boundary](./work-items/W7-platform-boundary.md)
- [W8: Workspace Projection And UX](./work-items/W8-workspace-projection-ux.md)

Sub-agent 派发规范与并行策略见 [subagent-dispatch.md](./subagent-dispatch.md)。

## Optimal Execution Plan

最优方案按 4 个 wave 推进：

1. Wave 0 foundation: W0 source identity model 独占，随后 W1 mailbox intake command shape 独占。
2. Wave 1 independent backend paths: W2 与 W3 在 W1 后并行。
3. Wave 2 Companion delivery adapter: W4 在 W3 后执行，作为 gate resolve -> mailbox delivery 的样板。
4. Wave 3 remaining Companion surface: W5、W6 顺序执行；W7 可在不改 active Companion 文件时并行。
5. Wave 4 projection and checks: W8 在 W2-W6 后端路径稳定后执行。

这个方案最大化可并行性，同时避免 `companion/tools.rs`、`companion/gate_control.rs` 和 mailbox helper shape 的高冲突窗口。

## Cross-Item Guardrails

- [ ] Mailbox 保持 per-AgentRun durable inbox 与 scheduler。
- [ ] RoutineExecution、LifecycleGate、AgentRunMailboxMessage 三者职责不互相吞并。
- [ ] 所有新投递路径先有 durable mailbox envelope，再触发 scheduler。
- [ ] Companion gate 继续作为 request/review/wait/correlation 事实。
- [ ] Routine / Companion 不新增平行 pending queue。
- [ ] Source identity schema 必须通过 W0 的跨层 baseline 维护。
- [ ] Shared mailbox helper shape 必须通过 W1 维护。
