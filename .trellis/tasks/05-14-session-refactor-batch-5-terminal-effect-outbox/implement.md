# Implementation Plan：Batch 5 Terminal Effect Outbox

## Ordered Steps

- [x] 新增 `session/terminal_effects.rs`，定义 outbox record / effect type / status / dispatcher。
- [x] 扩展 `SessionPersistence` trait：insert pending、mark running、mark succeeded、mark failed、list pending/failed。
- [x] 实现 `MemorySessionPersistence` outbox 存储与单测。
- [x] 实现 SQLite `session_terminal_effects` 表、读写方法、删除级联与单测。
- [x] 实现 PostgreSQL 初始化与 migration `0034_session_terminal_effect_outbox.sql`。
- [x] 将 `SessionTurnProcessor` 终态副作用迁移到 dispatcher。
- [x] 更新现有 hub tests，并新增 terminal effect dispatcher focused tests。
- [x] `rg` 确认 processor 不再直接调用 terminal effect 副作用入口。

## Verification

```powershell
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-infrastructure
cargo test -p agentdash-application session::terminal_effects
cargo test -p agentdash-application session::hub
cargo test -p agentdash-application session::memory_persistence
cargo test -p agentdash-infrastructure session_repository
rg -n "execute_effects|on_session_terminal|request_hook_auto_resume" crates/agentdash-application/src/session/turn_processor.rs
```

## Exit Criteria

- terminal fact 写入与 terminal effect 执行解耦。
- outbox record 可持久化、可查询、可标记成功/失败。
- effect 执行失败不影响 session terminal status。
- processor 不再直接承载业务副作用分发。
