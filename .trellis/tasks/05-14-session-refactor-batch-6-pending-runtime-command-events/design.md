# Design：Batch 6 Pending Runtime Command Events

## Boundary

`SessionMeta` 只保留 session meta 与 event projection 字段。pending runtime command 属于独立事实流，由 `SessionPersistence` 暴露 store 方法。

## Data Model

```rust
pub enum RuntimeCommandStatus {
    Pending,
    Applied,
    Failed,
}

pub struct PendingRuntimeCommandRecord {
    pub id: Uuid,
    pub session_id: String,
    pub transition_id: String,
    pub phase_node: String,
    pub status: RuntimeCommandStatus,
    pub transition: PendingCapabilityStateTransition,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub applied_at_ms: Option<i64>,
    pub failed_at_ms: Option<i64>,
    pub last_error: Option<String>,
}
```

## Flow

```text
workflow runtime context change while no live turn
  -> enqueue_pending_runtime_context_transition
  -> upsert_pending_runtime_command(session_id, transition)

next prompt
  -> list_pending_runtime_commands(session_id)
  -> build LaunchExecution with pending count / latest state
  -> apply_pending_runtime_context_transitions_on_turn(records)
  -> mark_runtime_commands_applied(ids)
```

失败路径：

- command store 写入失败：调用方返回错误，不伪造 pending。
- apply frame 持久化失败：本批保持现有 best-effort 行为，但 command 标记 applied 只发生在 apply 函数返回后。
- 后续 Batch 可将 apply frame 失败升级为 failed command。

## Persistence

新增 `session_runtime_commands` 表：

- `id TEXT PRIMARY KEY`
- `session_id TEXT NOT NULL`
- `transition_id TEXT NOT NULL`
- `phase_node TEXT NOT NULL`
- `status TEXT NOT NULL`
- `payload_json TEXT NOT NULL`
- `created_at_ms INTEGER NOT NULL`
- `updated_at_ms INTEGER NOT NULL`
- `applied_at_ms INTEGER`
- `failed_at_ms INTEGER`
- `last_error TEXT`

唯一索引：`(session_id, phase_node, status)` 仅 pending 需要唯一。SQLite/Postgres 对部分唯一索引写法不同，本批可以在 upsert 前显式把同 phase pending 标记 failed/superseded 或删除旧 pending，再插入新 pending。

## Migration Strategy

1. 增加 runtime command 类型与 persistence trait。
2. 实现 Memory store。
3. 实现 SQLite/Postgres 表与 migration。
4. 从 `SessionMeta` 删除 pending queue 字段。
5. 迁移 enqueue/apply/prompt pipeline。
6. 更新 tests 和 grep。
