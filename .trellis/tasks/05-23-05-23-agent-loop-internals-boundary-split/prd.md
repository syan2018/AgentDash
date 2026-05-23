# Agent loop internals 边界拆分

## Goal

将 agent_loop.rs 内部按 turn、tool_call、event_mapping、cancellation、prompt、output 逐步拆分，保持 runtime streaming 行为稳定。

## Requirements

- 将 `agentdash-agent/src/agent_loop.rs` 按 turn、tool_call、event_mapping、cancellation、prompt、output 等内部职责逐步拆分。
- 保持 connector-facing 行为、streaming event 映射和 tool approval 语义稳定。
- 优先移动纯 helper 与 event mapping，再移动有副作用的执行状态机。
- 每批移动后运行 `cargo check -p agentdash-agent -p agentdash-executor -p agentdash-application` 和 agent loop 相关测试。

## Acceptance Criteria

- [ ] 至少一个 agent loop 内部职责拆出独立模块。
- [ ] streaming/tool/cancel 既有测试或 check 通过。
- [ ] public API 不暴露 agent loop 内部实现模块。
- [ ] spec 或 review note 记录拆分顺序。

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
