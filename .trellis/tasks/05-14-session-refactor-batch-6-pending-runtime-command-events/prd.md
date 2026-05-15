# Session refactor batch 6 pending runtime command events

## Goal

删除 `SessionMeta.pending_capability_state_transitions` 这个隐形队列，将 runtime context / capability transition 改为独立持久化 runtime command：requested / applied / failed 可审计、可查询、可恢复。

## Current Fact

- `PendingCapabilityStateTransition` 当前藏在 `SessionMeta.pending_capability_state_transitions`。
- launch 前通过 `std::mem::take` 取走 pending 列表，随后 `save_session_meta` 顺带清空。
- 这个模型缺少 apply-once 事实、失败恢复、独立查询和审计；也让 `SessionMeta` 同时承担 meta 与 command queue。

## Requirements

- 新增 pending runtime command store，承载 capability state transition。
- command record 至少包含 `id`、`session_id`、`transition_id`、`phase_node`、`status`、`payload`、`created_at_ms`、`updated_at_ms`、`applied_at_ms`、`failed_at_ms`、`last_error`。
- 写入 pending transition 时按 `session_id + phase_node` 去重：同一 phase node 只保留最新 pending command。
- prompt pipeline 从 command store 查询 pending commands，不再读取 `SessionMeta` 队列。
- pending transition 成功应用后标记 `applied`；失败时标记 `failed` 并保留错误。
- `SessionMeta.pending_capability_state_transitions` 字段从应用层类型和 repository 映射中删除。
- SQLite / PostgreSQL schema 初始化与 migration 同步；删除 session 时 command 记录级联删除。

## Non-goals

- 不改变 runtime context transition 的业务 payload。
- 不引入通用命令总线；本批只处理 pending capability state transition。
- 不做 SessionHub 最终删除；那是 Batch 7。

## Acceptance Criteria

- [ ] 应用层 `SessionMeta` 不再包含 `pending_capability_state_transitions`。
- [ ] prompt pipeline 不再 `mem::take` meta 上的 pending queue。
- [ ] runtime context transition enqueue / apply 走 persistence command store。
- [ ] Memory / SQLite / PostgreSQL 均支持 requested / applied / failed 状态。
- [ ] 现有 pending transition hub test 通过并验证 command applied 状态。
- [ ] `rg "pending_capability_state_transitions" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src` 在生产代码中零命中。
