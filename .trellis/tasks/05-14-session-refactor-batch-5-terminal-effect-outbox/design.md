# Design：Batch 5 Terminal Effect Outbox

## Boundary

`SessionTurnProcessor` 保留 per-turn event 消费职责：

- persist normal notification。
- resolve terminal kind/message。
- persist terminal event。
- clear active turn。

终态后的业务动作转交给 `SessionTerminalEffectDispatcher`。dispatcher 负责：

- 根据 terminal outcome 与 hook session 构造 terminal effect records。
- 先写 durable outbox pending record。
- 调用当前进程内可用的 executor。
- 将执行结果写回 outbox status。

`SessionPersistence` 是 outbox 的存储边界；Memory / SQLite / PostgreSQL 必须同构。

## Data Model

```rust
pub enum TerminalEffectType {
    HookEffects,
    SessionTerminalCallback,
    HookAutoResume,
}

pub enum TerminalEffectStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}

pub struct TerminalEffectRecord {
    pub id: Uuid,
    pub session_id: String,
    pub turn_id: String,
    pub terminal_event_seq: u64,
    pub effect_type: TerminalEffectType,
    pub payload: serde_json::Value,
    pub status: TerminalEffectStatus,
    pub attempt_count: u32,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub last_error: Option<String>,
}
```

`terminal_event_seq` 将 outbox record 与已经持久化的 terminal fact 绑定，避免 effect 记录漂浮在事实之外。

## Effect Types

### HookEffects

Payload：

```json
{
  "effects": [HookEffect],
  "supported_effect_kinds": ["task:set_status"]
}
```

执行器：当前进程内的 `DynPostTurnHandler`。若 handler 不存在但 payload 有 effects，record 标记 failed，错误说明缺少 handler。

### SessionTerminalCallback

Payload：

```json
{ "terminal_state": "completed|failed|interrupted" }
```

执行器：`SessionHub.terminal_callback` 中当前可用 callback。缺少 callback 时不创建 record；创建 record 后执行失败则 failed。

### HookAutoResume

Payload：

```json
{ "reason": "before_stop_continue" }
```

执行器：`SessionHub.request_hook_auto_resume(session_id)`。返回 false 视为 succeeded with no-op，payload 不再反向修改。

## Dispatcher Order

```text
processor receives terminal
  -> persist terminal notification
  -> collect terminal hook effects
  -> build effect plan
  -> for each effect:
       insert pending outbox record
       mark running / increment attempt
       execute effect
       mark succeeded or failed
  -> clear active turn
```

Hook `SessionTerminal` trigger 评估仍在 dispatcher 中同步执行，因为它产出 effect plan；它不是 durable outbox effect 本身。若 hook trigger 评估失败或无 hook session，则只是不产生 hook effects record。

## Persistence

新增 `session_terminal_effects` 表：

- `id TEXT PRIMARY KEY`
- `session_id TEXT NOT NULL`
- `turn_id TEXT NOT NULL`
- `terminal_event_seq INTEGER NOT NULL`
- `effect_type TEXT NOT NULL`
- `payload_json TEXT NOT NULL`
- `status TEXT NOT NULL`
- `attempt_count INTEGER NOT NULL DEFAULT 0`
- `created_at_ms INTEGER NOT NULL`
- `updated_at_ms INTEGER NOT NULL`
- `last_error TEXT`
- FK `session_id -> sessions(id) ON DELETE CASCADE`

索引：

- `(status, updated_at_ms)`
- `(session_id, turn_id)`
- `(session_id, terminal_event_seq)`

SQLite 初始化和 PostgreSQL 初始化都直接创建表；PostgreSQL migrations 增加 `0034_session_terminal_effect_outbox.sql`。

## Failure Semantics

- terminal event append 成功后，effect 失败只更新 outbox，不回滚 terminal event。
- dispatcher 捕获每个 effect 的错误，继续处理后续 effect。
- attempt_count 在每次执行前递增。
- `Failed` record 保留 `last_error`，后续 replay 可重新 claim pending/failed。

## Migration Strategy

1. 增加 outbox types 与 persistence trait 方法，先让 Memory repository 测试通过。
2. 增加 SQLite / PostgreSQL repository 表、序列化和状态更新。
3. 新增 dispatcher，将 processor 中直接副作用搬进去。
4. 更新 hub tests 与新增 dispatcher focused tests。
5. 跑 application/api/infrastructure 相关检查。
