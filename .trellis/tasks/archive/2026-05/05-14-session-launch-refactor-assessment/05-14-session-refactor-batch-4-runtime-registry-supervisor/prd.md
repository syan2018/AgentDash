# Session refactor batch 4 runtime registry supervisor

## Goal

拆分 SessionRuntimeRegistry 与 TurnSupervisor，区分 live executor session / active turn，并迁移 cancel/stall/delete 运行态入口。

## Current Fact

- `SessionHub` 仍直接持有 `sessions: Arc<Mutex<HashMap<String, SessionRuntime>>>`。
- `SessionRuntime` 同时承载 session 级 profile/hook metadata 与 turn 级 `TurnState`。
- `cancel` / stall detector / hook runtime sink / prompt pipeline / runtime context transition 都直接锁 `hub.sessions`。
- `has_live_runtime` 当前语义实际接近“有内存 runtime entry”，不是“connector 有 live executor session”，容易和 active turn 混用。

## Requirements

- 新增 `SessionRuntimeRegistry`，作为 `SessionRuntime` 内存态唯一访问入口。
- 新增或明确 `TurnSupervisor` 边界，集中处理 turn claim / activate / processor_tx 注册 / terminal cleanup / cancel flag / stalled scan。
- 明确区分：
  - `has_runtime_entry(session_id)`：hub 内存中是否有 runtime 记录。
  - `has_active_turn(session_id)`：是否有当前 active turn。
  - `has_live_executor_session(session_id)`：connector 层是否存在 live executor session。
- `SessionHub` 不再把 `sessions` 字段直接暴露给各模块；模块应通过 registry/supervisor 方法完成读写。
- `cancel` 与 `stall_detector` 不再手写 `runtime.turn_state` 分支；通过 supervisor 请求取消并记录 interrupted 兜底事件。
- hook runtime injection sink、runtime context transition、tool builder 需要通过 registry/supervisor 查询/更新 active turn，而不是直接操作 map。
- 保持现有行为：并发 prompt 只能一个 claim 成功；cancel 可触发 connector.cancel 并在 stream 断开时记录 interrupted；stall detector 仍取消超时 active turn。

## Non-goals

- 不拆 terminal effect outbox。
- 不迁移 pending runtime command 持久化。
- 不删除 `SessionHub` 外部 API；本批只切内部运行态边界。
- 不改变 connector prompt/cancel 协议。

## Acceptance Criteria

- [ ] 新增 runtime registry / supervisor 模块与 focused tests。
- [ ] `SessionHub` 字段不再直接暴露 `sessions` map；除 registry/supervisor 内部和测试 fixture 外，生产代码不直接锁 runtime map。
- [ ] `has_live_runtime` 被替换或重命名为语义准确的方法，调用点按真实意图迁移。
- [ ] prompt pipeline 的 turn claim/activate/cleanup 通过 supervisor 完成。
- [ ] cancel/stall 路径通过 supervisor 完成 active turn 取消。
- [ ] hook/runtime context/tool update 等路径通过 registry/supervisor 投影 active runtime。
- [ ] 现有 session hub / cancel / stall / hook auto-resume / runtime context transition 测试通过。

## Notes

- 本批依赖 Batch 3 的 `LaunchCommand` / `PreparedLaunchPrompt` 收口；不要回退到入口层 request 分支。
