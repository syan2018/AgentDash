# Pi Agent 完整 Hook Runtime v2 计划总任务

## Goal

将当前已落地的 Hook Runtime 主干，继续推进为我们真正需要的完整机制：支持多来源声明、动态上下文注入、工具审批/改写、companion 上下文切片与结果回流，并以最终全链路联调作为完成标识。

## Background

当前项目已经完成了：

- Hook Runtime 主干
- workflow completion 闭环
- companion dispatch 第一版生命周期
- hook trace/debug surface
- deny/rewrite/stop gate 基础回归

但这些仍然只是第一阶段能力。我们后续真正要实现的是：

- 类 Claude Code 的动态 hook 上下文与行为约束
- workflow 只作为声明层 / 配置层，不重新长成硬编码引擎
- companion/subagent 真正成为平台级行为，而不是 prompt 拼接技巧

## Scope

- 建立 Hook v2 的来源模型、优先级与合并策略
- 建立 companion context slicing / inheritance downgrade / result return channel
- 建立 Ask/Approval/Resume 正式交互链路
- 打通前后端、trace、session、companion 的最终联调闭环

## Milestones

- `03-21-hook-v2-source-merge-and-governance`
- `03-21-hook-v2-companion-context-slicing-and-return-channel`
- `03-21-hook-v2-ask-approval-and-resume-flow`
- `03-21-hook-v2-full-e2e-joint-debugging`

## Requirements

- 不把新的 hook 需求继续散写进 `execution_hooks.rs` 的特化 if/else
- workflow / builtin workflow 继续保持“声明层、来源层”定位
- Hook 控制面继续通过 loop 边界同步决策，不把业务查询塞回 `agent_loop`
- 所有新增行为都必须能在 runtime trace / frontend surface 中可观测

## Completion Rule

这个总任务不能以“代码大体写完”作为完成标准。

只有满足以下条件，才允许标记为 `completed`：

- 4 个分任务全部完成
- 浏览器 / 会话 / companion 的最终联调任务完成
- 可以明确演示一条完整链路：
  - 主 session 加载多来源 hook
  - 命中 tool gate / ask / rewrite / stop control
  - companion dispatch 使用切片后的上下文启动
  - companion 结果回流主 session
  - 前端 trace/debug surface 可以看到整条链

## Acceptance Criteria

- [ ] Hook v2 来源模型、优先级与合并策略已稳定落地
- [ ] companion 上下文切片、继承降级与结果回流已打通
- [ ] Ask/Approval/Resume 已具备正式交互链路
- [ ] 至少 1 条完整主 session -> hook -> companion -> 回流 -> trace 的联调链路跑通
- [ ] 本总任务只在最终联调 task 完成后才可标记为 completed

## References

- [execution-hook-runtime.md](/F:/Projects/AgentDash/.trellis/spec/backend/execution-hook-runtime.md)
- [execution_hooks.rs](/F:/Projects/AgentDash/crates/agentdash-api/src/execution_hooks.rs)
- [hub.rs](/F:/Projects/AgentDash/crates/agentdash-executor/src/hub.rs)
- [address_space_access.rs](/F:/Projects/AgentDash/crates/agentdash-api/src/address_space_access.rs)
- [SessionPage.tsx](/F:/Projects/AgentDash/frontend/src/pages/SessionPage.tsx)
