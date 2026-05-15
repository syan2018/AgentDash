# Design：Batch 4 Runtime Registry 与 Turn Supervisor

## Boundary

`SessionRuntimeRegistry` 只负责 session runtime map 的存取、投影和受控更新；它不调用 connector，不写 terminal event，不理解业务 owner。

`TurnSupervisor` 负责 turn 生命周期状态机：

- claim：从 idle 进入 claimed，拒绝并发 prompt。
- activate：注册 `TurnExecution`、processor_tx、capability_state、context。
- cancel：标记 active turn cancel_requested，并返回是否需要调用 connector.cancel。
- cleanup：turn terminal 后清理 active turn、更新 profile/hook session。
- stalled scan：按 last_activity_at 找 active turn。

`SessionHub` 保留外部协调入口，但不直接操作 runtime map。prompt pipeline / cancel / stall / hook sink 逐步改用 registry/supervisor。

## Data Flow

```text
launch_command
  -> prompt_pipeline
  -> turn_supervisor.claim(session_id)
  -> build LaunchExecution
  -> turn_supervisor.activate(session_id, turn)
  -> connector.prompt
  -> turn_processor
  -> turn_supervisor.observe / cleanup
```

## Method Shape

首批实现以小步迁移为准，允许 registry/supervisor 持有同一把 `Arc<Mutex<HashMap<...>>>`，但只有新类型内部能锁 map。

```rust
pub(super) struct SessionRuntimeRegistry {
    runtimes: Arc<Mutex<HashMap<String, SessionRuntime>>>,
}

pub(super) struct TurnSupervisor {
    registry: SessionRuntimeRegistry,
}
```

必要投影方法：

- `ensure_runtime(session_id)`.
- `has_runtime_entry(session_id)`.
- `has_active_turn(session_id)`.
- `active_turn_snapshot(session_id)`.
- `with_active_turn_mut(session_id, f)`.
- `mark_activity(session_id, turn_id)`.
- `find_stalled_active_turns(timeout_ms)`.

## Migration Strategy

1. 引入 registry/supervisor，先包住现有 map，不改变行为。
2. 迁移只读调用点：`has_live_runtime` / active runtime 检查 / stalled scan。
3. 迁移 prompt pipeline claim/activate/processor_tx 注册。
4. 迁移 cancel 与 stream terminal cleanup。
5. 收紧 `SessionHub` 字段可见性，删除生产代码直接锁 map。

## Risks

- turn cleanup 顺序错误会导致并发 prompt 永久被拒。
- cancel flag 丢失会让 interrupted 事件退化为 failed。
- hook/runtime context transition 如果拿不到 active turn，会丢 runtime injection。

这些风险必须由现有 hub tests 和新增 focused tests 覆盖。
