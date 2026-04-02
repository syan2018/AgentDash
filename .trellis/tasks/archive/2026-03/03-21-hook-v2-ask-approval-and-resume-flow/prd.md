# Hook v2 Ask 审批与恢复执行链路

## Goal

把当前 Hook 决策模型中的 `Ask` 从枚举能力推进为正式交互链路，使工具审批、人工确认、恢复执行与 trace 记录成为平台级能力。

## Scope

- Ask decision runtime contract
- 审批 surface / 恢复执行
- 审批后的 tool replay / continue
- diagnostics / trace / frontend state

## Requirements

- Ask 不能只停在后端决策枚举里
- 审批通过/拒绝必须形成正式状态机
- 恢复执行需要与当前 session / turn / tool call 对齐
- 前端必须能看见 pending approval 与处理结果

## Acceptance Criteria

- [ ] Ask 决策已打通到前端交互面
- [ ] 已支持 approve / reject / resume
- [ ] 已能把审批结果写入 trace / diagnostics
- [ ] 至少 1 条需要人工确认的工具路径可真实演示
- [ ] Ask 不会破坏现有 deny/rewrite/stop gate 逻辑

## References

- [types.rs](crates/agentdash-agent/src/types.rs)
- [runtime_delegate.rs](crates/agentdash-executor/src/runtime_delegate.rs)
- [SessionPage.tsx](frontend/src/pages/SessionPage.tsx)
