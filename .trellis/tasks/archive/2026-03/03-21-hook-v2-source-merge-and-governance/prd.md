# Hook v2 来源合并与治理

## Goal

把 Hook 的来源从“当前已收敛的一批规则”进一步推进为正式多层来源模型，支持全局 builtin、workflow、project、story、task、session 等层级的统一声明、优先级与合并治理。

## Scope

- 定义 Hook source model
- 定义 precedence / override / merge 策略
- 定义 source summary / traceability contract
- 让 workflow 继续保持声明层，不再承担执行引擎角色

## Requirements

- 不允许把更多业务分支继续直写进 `execution_hooks.rs`
- 所有 Hook 规则都要有来源与优先级解释
- 前端 trace/debug surface 必须能看出规则来自哪一层

## Acceptance Criteria

- [ ] 已定义 Hook 多来源声明模型
- [ ] 已打通至少全局 builtin + workflow + task 三层合并
- [ ] 已明确 precedence / override / merge 行为
- [ ] runtime trace / diagnostics 能解释规则来源
- [ ] workflow 仍然只承担声明层与信息来源职责

## References

- [execution-hook-runtime.md](.trellis/spec/backend/execution-hook-runtime.md)
- [execution_hooks.rs](crates/agentdash-api/src/execution_hooks.rs)
- [trellis_dev_task.json](crates/agentdash-application/src/workflow/builtins/trellis_dev_task.json)
