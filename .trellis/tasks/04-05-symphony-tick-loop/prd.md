# Symphony Milestone: Project Agent 定时触发基础设施

## 目标

> **重要重定位**: 此 task 原名"Orchestrator Tick Loop 核心调度循环"。根据设计对账 #1，
> AgentDash 不需要一个刚性的 poll-dispatch daemon。真正需要的是：**让 Project Agent 能被
> 定时事件唤醒**，由 agent 自己决定做什么。

为 Project Agent 提供周期性触发能力。Agent 被唤醒后，通过已有的 tool 体系（project 状态查询、
task 管理、session 创建）自主决定下一步行动。

## 核心行为

```
定时器触发 → 唤醒 Project Agent session
  → Agent 评估项目当前状态（pending tasks? running sessions? story 进展?）
  → Agent 自主决策（dispatch new task? continue existing? skip this tick?）
  → 若需要执行，调用现有 task/session API
```

## 与 Symphony Tick Loop 的本质区别

| 维度 | Symphony | AgentDash (目标) |
|------|----------|----------------|
| 调度逻辑 | 硬编码在 orchestrator 中 | 由 Agent prompt/workflow 定义 |
| Candidate 选择 | 固定排序规则 | Agent 根据上下文智能判断 |
| Dispatch 决策 | 机械式填充槽位 | Agent 权衡优先级、依赖、资源 |
| 灵活性 | 修改需改代码 | 修改 workflow/prompt 即可 |

## 核心需求

1. **定时触发器**: 可配置的周期性唤醒机制（`poll_interval_ms`）
2. **Agent Session 管理**: 确保 Project Agent session 在触发时能正确恢复/创建
3. **上下文注入**: 触发时向 Agent 提供当前项目状态摘要
4. **生命周期**: 触发器的 start/stop/pause 控制

## 待讨论

- [ ] 定时触发是否复用现有 session follow-up 机制，还是需要独立的 timer 基础设施？
- [ ] Project Agent 的 session 生命周期：每次触发创建新 session？还是维持一个长期 session？
- [ ] 触发时的上下文注入策略：是一个 system prompt with current state，还是通过 tool call 让 agent 自行查询？
- [ ] 与 Lifecycle 的关系：定时触发是否应该由 lifecycle step transition 规则控制？
- [ ] 如何处理"上一个 tick 的 agent 还在跑"的重入问题？

## 依赖

- symphony-orchestrator-config（poll_interval_ms 配置）
- symphony-concurrency-governor（Agent 需要查询可用槽位）

## 参考

- Symphony spec §8.1 (Poll Loop) — 参考其 tick 序列但用 agent 替代硬编码逻辑
- Symphony spec §16.2 (Poll-and-Dispatch Tick)
- `crates/agentdash-application/src/session_plan.rs` — `SessionPlanPhase::ProjectAgent`
- `crates/agentdash-application/src/companion/tools.rs` — Companion 工具体系
