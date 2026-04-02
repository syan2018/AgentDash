# Hook v2 companion 上下文切片与结果回流

## Goal

把当前第一版 `companion_dispatch` 从“默认全量继承 runtime context/constraints”升级为正式 companion 机制，支持上下文切片、继承降级、结构化结果回流与主 session 采纳策略。

## Scope

- companion context slicing
- inheritance downgrade
- result return channel
- 主 session 对 companion 结果的消费策略

## Requirements

- companion 不应默认无差别继承全部上下文
- 切片策略必须可解释、可追踪
- companion 结果不能只停留在 trace，需要能正式回流主 session
- 不允许重新退化成“把一大段 prompt 丢给 companion”

## Acceptance Criteria

- [ ] 已定义 companion context slicing 规则
- [ ] 已支持至少按 source/slot/tag 的继承过滤
- [ ] 已支持 companion 结构化结果回流主 session
- [ ] 主 session 能明确看到并决定是否采纳 companion 结果
- [ ] 前端或 trace 面可观测 companion dispatch 与回流链路

## References

- [address_space_access.rs](crates/agentdash-api/src/address_space_access.rs)
- [execution_hooks.rs](crates/agentdash-api/src/execution_hooks.rs)
- [hub.rs](crates/agentdash-executor/src/hub.rs)
