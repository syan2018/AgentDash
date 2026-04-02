# Hook runtime trace 与调试面板

## Goal

为 Hook Runtime 建立面向调试与运行可观测性的 trace surface，让开发者能看到每次 trigger 命中了什么、为何 block/rewrite/refresh，以及 snapshot revision 如何演进。

## Background

当前会话页已经能展示：

- runtime snapshot
- active workflow metadata
- diagnostics
- policies / constraints

但它仍然偏静态，缺少“本轮实际发生了什么”的运行态追踪：

- 哪个 trigger 命中了哪些 rule
- 哪次 tool call 被 block / rewrite
- 为什么 refresh_snapshot
- diagnostics 属于 snapshot 基线还是本轮 resolution

## Scope

- 定义 hook trace entry 结构
- 补 API 暴露形式
- 在 Session 页面增加 trace/debug surface
- 区分 snapshot baseline 与 per-trigger resolution

## Requirements

- 追踪信息不能侵入 agent_loop 业务依赖
- trace entry 必须带 trigger / decision / source summary / revision
- 前端展示要适合日常调试，不只是 dump 原始 JSON
- 支持 session 级增量查看

## Acceptance Criteria

- [ ] 明确 trace entry 数据结构
- [ ] API 能返回最近一段 hook trigger 记录
- [ ] 前端能展示 trigger 命中、决策、刷新原因
- [ ] 能区分 snapshot 基线 diagnostics 与 resolution diagnostics
- [ ] 至少有 1 条真实交互链路可在页面中观察完整 trace

## References

- [SessionPage.tsx](frontend/src/pages/SessionPage.tsx)
- [services/session.ts](frontend/src/services/session.ts)
- [execution_hooks.rs](crates/agentdash-api/src/execution_hooks.rs)
- [hooks.rs](crates/agentdash-executor/src/hooks.rs)
