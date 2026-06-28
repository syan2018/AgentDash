# W0: Source And Schema Baseline

Status: planned

## Goal

拉平 mailbox source 在 migration、domain、API mapper、contract DTO、generated TS 与 frontend labels 的一致性，为 Routine / Companion 新 source 提供稳定基础。

## Dependencies

无前置工作项。W0 是 W1-W8 的 schema 前置条件。

## Deliverables

- [ ] 修正 `agent_run_mailbox_messages_source_check` 与 domain/API source enum 的一致性，至少包含当前代码已使用的 `canvas_action`。
- [ ] 新增或规范 Companion source：`companion_dispatch`、`companion_result`、`companion_parent_request`、`companion_parent_response`、`companion_human_response`。
- [ ] 更新 API mapper、contract DTO、generated TS、frontend label 映射。
- [ ] 增加 source enum / migration drift 测试。

## Acceptance

- [ ] Migration、domain enum、API mapper、generated TS source union 完全一致。
- [ ] Canvas submit 现有行为不再受 migration check constraint 阻断。
- [ ] 后续 Routine / Companion source 新增有单一跨层检查入口。

## Suggested Validation

- `cargo test -p agentdash-domain agent_run_mailbox`
- `cargo test -p agentdash-infrastructure agent_run_mailbox`
- `pnpm run contracts:check`

## Parallel Guidance

W0 必须独占执行。其它实现工作项只能在 W0 合并后开始，避免 source value 和 contract churn 反复冲突。

