# Implementation Plan：Batch 6 Pending Runtime Command Events

## Ordered Steps

- [x] 新增 `session/runtime_commands.rs` 类型。
- [x] 扩展 `SessionPersistence` runtime command 方法。
- [x] 实现 Memory / SQLite / PostgreSQL command store 与 migration。
- [x] 删除 `SessionMeta.pending_capability_state_transitions` 字段与 repository 映射。
- [x] 迁移 `enqueue_pending_capability_state_transition` 到 command store。
- [x] 迁移 prompt pipeline pending transition 查询与 applied 标记。
- [x] 更新 pending transition hub test 与 repository focused tests。
- [x] `rg` 确认旧 meta queue 主链路零命中。

## Verification

```powershell
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-infrastructure
cargo test -p agentdash-application session::runtime_commands
cargo test -p agentdash-application session::hub::tests::pending_capability_state_transition_applies_on_next_prompt_and_clears_meta
cargo test -p agentdash-infrastructure runtime_command
rg -n "pending_capability_state_transitions" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
```

## Exit Criteria

- pending runtime command 不再藏在 SessionMeta。
- command 状态可审计。
- 下轮 prompt apply 后 command 变为 applied。
