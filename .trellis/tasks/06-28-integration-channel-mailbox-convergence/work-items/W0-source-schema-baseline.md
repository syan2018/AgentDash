# W0: Mailbox Source Model Baseline

Status: done

## Goal

重建 mailbox source identity / envelope attribution 模型，避免继续用 closed enum 和 DB check constraint 表达来源。该模型要支撑当前 core/Routine/Companion 来源，并为未来 Agent channel / integration adapter 留出开放边界。

## Dependencies

无前置工作项。W0 是 W1-W8 的 schema 前置条件。

## Deliverables

- [x] 设计并落库开放式 `MailboxSourceIdentity`，至少包含 namespace、kind、source_ref、correlation_ref、actor、route metadata、display_label_key、metadata/source_metadata。
- [x] 将当前 `MailboxMessageSource` enum/check constraint 迁移为 source identity；`canvas_action` drift 在迁移中自然消除。
- [x] 更新 domain、repository、API mapper、contract DTO、generated TS、frontend fixture，前端不再依赖 closed source union 扩展业务来源。
- [x] 明确 scheduler 不按 source identity 决定 delivery；delivery 继续由 origin/delivery/barrier/drain_mode/runtime state 驱动。
- [x] 增加 source identity serialization / repository / contract drift 测试。

## Acceptance

- [x] Migration、domain model、API mapper、generated TS 对 source identity 字段完全一致。
- [x] Canvas submit 现有行为不再受 source check constraint 阻断。
- [x] Routine / Companion source 不需要新增 enum variant 即可表达。
- [x] 后续 channel / integration adapter 能通过 namespace/kind/source_ref/metadata 接入 attribution，不需要改 scheduler 分支。

## Suggested Validation

- `cargo test -p agentdash-domain agent_run_mailbox`
- `cargo test -p agentdash-infrastructure agent_run_mailbox`
- `pnpm run contracts:check`

## Parallel Guidance

W0 必须独占执行。其它实现工作项只能在 W0 合并后开始，避免 source identity schema 和 contract churn 反复冲突。
