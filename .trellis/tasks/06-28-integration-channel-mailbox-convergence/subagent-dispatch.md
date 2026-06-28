# Sub-agent Dispatch Guide

## Dispatch Model

本任务不创建 Trellis 子任务。Sub-agent 只领取当前 task 下的工作项文件，并在当前 task 中回报进度、风险和验证结果。

主会话职责：

- 维护 `prd.md`、`design.md`、`implement.md` 和工作项状态。
- 派发 sub-agent 前确认依赖已满足。
- 合并跨工作项设计决策，尤其是 mailbox helper shape、source identity schema、gate payload refs。
- 处理跨文件冲突和最终检查。

Sub-agent 职责：

- 只实现被派发工作项的 deliverables。
- 开始前完整阅读 `implement.jsonl` 中列出的 spec/task docs，以及当前 task 的 `prd.md`、`design.md`、`implement.md`、`subagent-dispatch.md` 和被派发的 `work-items/W*.md`。
- 不允许只根据工作项摘要开工；必须掌握完整设计语境，尤其是 mailbox source identity、per-AgentRun scheduler 边界、RoutineExecution / LifecycleGate / AgentRunMailboxMessage 职责拆分。
- 不修改其它工作项状态，除非主会话明确授权。
- 不创建 Trellis 子任务，不切换 active task。
- 不引入 direct launch fallback、runtime-only notification delivery 或新的 pending queue。
- 完成后汇报改动文件、验证命令、未覆盖风险和需要主会话合并的接口。

## Design Context Gate

Sub-agent 执行前必须确认已经完整读完：

- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/prd.md`
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/design.md`
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/implement.md`
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/subagent-dispatch.md`
- 对应 `work-items/W*.md`

如果 sub-agent 不能复述本任务的核心设计边界，主会话不得接受其实现结果。核心边界包括：

- Mailbox 是 per-AgentRun durable inbox / scheduler，不是全局 channel broker。
- Source identity 是开放 attribution/correlation/projection 模型，不是继续追加 closed enum。
- Scheduler 不按 source identity 分支，仍按 origin、delivery、barrier、drain_mode、priority 和 runtime state 调度。
- RoutineExecution 和 LifecycleGate 保留业务事实；AgentRunMailboxMessage 承担投递事实。

## Required Context Per Agent

所有 sub-agent 必读：

- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/prd.md`
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/design.md`
- 对应 `work-items/W*.md`

Companion 相关 sub-agent 还应重点走读：

- `crates/agentdash-application/src/companion/tools.rs`
- `crates/agentdash-application/src/companion/dispatch.rs`
- `crates/agentdash-application/src/companion/gate_control.rs`
- `crates/agentdash-application/src/companion/notifications.rs`

Routine 相关 sub-agent 还应重点走读：

- `crates/agentdash-application/src/routine/executor.rs`
- `crates/agentdash-application/src/routine/dispatch.rs`
- `crates/agentdash-application/src/routine/reuse_resolver.rs`
- `crates/agentdash-domain/src/routine/entity.rs`

Mailbox 相关 sub-agent 还应重点走读：

- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs`
- `crates/agentdash-application-agentrun/src/agent_run/mailbox.rs`
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/`（W0A 完成后）
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs`
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql`
- `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs`

## Parallel Feasibility

| Item | Can Parallelize | Reason |
| --- | --- | --- |
| W0 | No | Defines source identity / envelope attribution model used by every later item. |
| W0A | No | Moves `agent_run/mailbox` into a directory module; every later mailbox helper change depends on the new file ownership. |
| W1 | No | Defines shared mailbox intake helper shape; parallel implementation would duplicate wrappers. |
| W2 | Yes, after W1 | Mostly Routine files plus mailbox helper call sites. Can run with W3. |
| W3 | Yes, after W1 | Companion child dispatch path. Can run with W2, but should precede W4. |
| W4 | Limited | Establishes Companion gate-to-mailbox delivery pattern. Should run before W5/W6 implementation. |
| W5 | Limited | Shares `gate_control.rs` / `tools.rs` with W6; design can parallelize, code should sequence. |
| W6 | Limited | Shares `gate_control.rs` / `tools.rs` with W5; code should sequence unless strict file ownership is agreed. |
| W7 | Yes | Mostly boundary/test/spec. Avoid concurrent edits to `companion/tools.rs`. |
| W8 | Partial | Contract/label prep after W0; final projection after W2-W6. |

## Optimal Execution Plan

Wave 0: Foundation

- Run W0 source identity model alone.
- Run W0A agent_run/mailbox directory split alone after W0.
- Run W1 mailbox intake command shape alone after W0A.

Wave 1: Independent backend paths

- Run W2 and W3 in parallel after W1.
- Keep W2 scoped to Routine files and mailbox helper usage.
- Keep W3 scoped to Companion child dispatch and child mailbox launch.

Wave 2: Companion delivery adapter

- Run W4 after W3. This establishes the first gate resolve -> mailbox delivery pattern.
- Main session reviews W4 before W5/W6 so parent/human paths reuse the same helper and payload ref style.

Wave 3: Companion remaining interaction surface

- Run W5 then W6 sequentially for lowest merge risk.
- If schedule pressure requires parallelism, assign W5 to parent-owned gate paths and W6 to human gate/respond paths, with the main session owning the final `companion/tools.rs` merge.
- Run W7 in parallel only if it does not modify active Companion files.

Wave 4: Projection and final checks

- Run W8 after W2-W6 backend message creation is stable.
- Final check agent runs mailbox, routine, companion, contracts and frontend checks listed in work items.

## Conflict Rules

- If two agents need to edit `crates/agentdash-application/src/companion/tools.rs`, pause one and let the main session serialize the edits.
- If a work item needs to change mailbox source identity schema, it must go back through W0 rather than adding local ad hoc strings or enum variants.
- If a work item needs a new mailbox helper field, it must update W1 first and notify active dependent agents.
- If a work item needs to move `agent_run/mailbox` files, it must go through W0A rather than mixing file moves into Routine or Companion delivery changes.
- If tests fail due to unrelated dirty workspace changes, do not revert them; report the scope and continue with targeted checks where possible.

## Completion Report Format

Each sub-agent should report:

```text
Work item:
Status:
Design context read:
Files changed:
Validation run:
Validation result:
Residual risks:
Follow-up needed from main session:
```
