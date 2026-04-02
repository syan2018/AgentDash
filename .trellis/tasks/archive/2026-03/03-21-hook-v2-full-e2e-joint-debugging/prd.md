# Hook v2 最终全链路联调

## Goal

把 Hook v2 的关键能力通过真实会话、真实前端、真实 companion 行为跑通，并把这项联调作为整个 Hook v2 计划的最终完成标识。

## Scope

- 浏览器 / 会话页联调
- hook trace / diagnostics / policy surface 联调
- tool deny / rewrite / ask / stop gate 联调
- companion dispatch / result return 联调

## Requirements

- 不接受只靠单测和 mock 判断“已经完成”
- 联调必须覆盖主 session、前端观测面、companion 行为
- 最终结论必须能说明哪些链路已跑通，哪些仍未覆盖

## Completion Rule

这是 Hook v2 计划的最终里程碑任务。

只有当以下链路被真实验证后，才允许将本任务与总任务标记完成：

- 主 session 实际加载多来源 hook
- 命中至少一条 deny/rewrite/ask/stop 中的多种控制路径
- companion dispatch 使用切片后的上下文成功启动
- companion 结果回流到主 session
- 前端 trace/debug surface 能完整看到这条链

## Acceptance Criteria

- [ ] 至少 1 条完整主 session -> hook -> companion -> 回流链路联调通过
- [ ] deny / rewrite / ask / stop 至少覆盖其中 3 类真实路径
- [ ] 前端会话页能正确展示 runtime snapshot / trace / diagnostics / companion 事件
- [ ] 联调结论形成明确记录，可作为总任务完成依据

## References

- [SessionPage.tsx](frontend/src/pages/SessionPage.tsx)
- [execution_hooks.rs](crates/agentdash-api/src/execution_hooks.rs)
- [address_space_access.rs](crates/agentdash-api/src/address_space_access.rs)
- [hub.rs](crates/agentdash-executor/src/hub.rs)
