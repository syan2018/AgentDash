# Sub-agent Dispatch Guide

## Dispatch Model

本任务不创建 Trellis 子任务。Sub-agent 只领取当前 task 下的工作项文件，并在当前 task 中回报进度、风险和验证结果。

主会话职责：

- 维护 `prd.md`、`design.md`、`implement.md` 和工作项状态。
- 派发 sub-agent 前确认依赖已满足。
- 合并跨工作项设计决策，尤其是 mailbox helper shape、source values、gate payload refs。
- 处理跨文件冲突和最终检查。

Sub-agent 职责：

- 只实现被派发工作项的 deliverables。
- 开始前读取 `implement.jsonl` 中列出的 spec/task docs，以及被派发的 `work-items/W*.md`。
- 不修改其它工作项状态，除非主会话明确授权。
- 不创建 Trellis 子任务，不切换 active task。
- 不引入 direct launch fallback、runtime-only notification delivery 或新的 pending queue。
- 完成后汇报改动文件、验证命令、未覆盖风险和需要主会话合并的接口。

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
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs`
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql`
- `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs`

## Parallel Feasibility

| Item | Can Parallelize | Reason |
| --- | --- | --- |
| W0 | No | Defines source/schema baseline used by every later item. |
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

- Run W0 alone.
- Run W1 alone after W0.

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
- If a work item needs to change mailbox source values, it must go back through W0 rather than adding local ad hoc strings.
- If a work item needs a new mailbox helper field, it must update W1 first and notify active dependent agents.
- If tests fail due to unrelated dirty workspace changes, do not revert them; report the scope and continue with targeted checks where possible.

## Completion Report Format

Each sub-agent should report:

```text
Work item:
Status:
Files changed:
Validation run:
Validation result:
Residual risks:
Follow-up needed from main session:
```

