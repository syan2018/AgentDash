# Pi Agent companion/subagent hook 生命周期

## Goal

为 AgentDash 建立正式的 companion/subagent hook 生命周期，使 Pi Agent 在外循环边界上能够像 Claude Code / Trellis 一样，对 subagent dispatch 做上下文切片、策略继承和运行态约束注入。

## Background

当前 Hook Runtime 已经完成：

- session 级 snapshot / diagnostics / revision
- `transform_context` / `before_tool_call` / `after_tool_call` / `after_turn` / `before_stop`
- workflow phase -> runtime policy / completion gate / 前端可视化

但 companion/subagent 仍然没有平台级生命周期抽象。当前系统还回答不了这些问题：

- subagent dispatch 在 runtime 上是什么事件
- 派发前由谁决定可继承的 hook/context
- 子 agent 应拿到 session 的哪一层裁剪视图
- dispatch 行为如何被 trace / diagnostics 记录

## Scope

- 定义 `before_subagent_dispatch` / `after_subagent_dispatch` 的正式语义
- 建立 subagent 上下文切片模型
- 定义主 agent -> subagent 的 policy 继承/降级/禁止规则
- 设计 dispatch trace / diagnostics / source summary 输出
- 为 Pi Agent 先提供第一版执行骨架

## Non-Goals

- 不在本任务内一次性实现所有 companion UX
- 不在本任务内引入完整 DSL
- 不把 subagent 行为重新写回 workflow 特化 prompt 拼接

## Requirements

- `agent_loop` 仍保持纯 runtime，不直接访问 repo / workflow / trellis
- 子 agent hook 信息必须由 executor/api 层在 loop 外准备
- dispatch 前必须能同步决策“是否允许派发、如何裁剪上下文”
- 子 agent 必须能知道自己的 owner / role / source summary
- 主 agent 与子 agent 的 hook trace 必须可以区分

## Acceptance Criteria

- [ ] 明确 subagent dispatch 的 trigger、输入、输出模型
- [ ] 明确主 agent snapshot 到子 agent snapshot 的切片规则
- [ ] 明确哪些 context / policy 允许继承，哪些必须降级或屏蔽
- [ ] 在 executor 层预留 subagent hook runtime 适配骨架
- [ ] 输出可落地的 diagnostics / trace 结构

## References

- [Pi Agent 动态 Hook 上下文与伴随 Agent 机制](/F:/Projects/AgentDash/.trellis/tasks/03-21-pi-agent-dynamic-hook-context/prd.md)
- [runtime_delegate.rs](/F:/Projects/AgentDash/crates/agentdash-executor/src/runtime_delegate.rs)
- [types.rs](/F:/Projects/AgentDash/crates/agentdash-agent/src/types.rs)
- [RUST_PI_HYBRID_DESIGN.md](/F:/Projects/AgentDash/crates/agentdash-agent/agent-design/RUST_PI_HYBRID_DESIGN.md)
- [Trellis README](/F:/Projects/AgentDash/references/Trellis/README.md)
