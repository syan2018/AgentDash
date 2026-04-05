# Symphony Milestone: 全局并发治理器

## 目标

为项目级 session 执行提供并发槽位管理。这不是一个独立的 dispatch 决策器，而是一个被 Project Agent（或任何 session 创建路径）查询的**约束层**。

## 架构定位重思

原始设计将此模块定位为"Orchestrator dispatch 前的硬性门控"。但根据设计对账 #1 的结论，dispatch 逻辑由 Project Agent 自身定义——平台层只提供**能力查询**和**强制上限**：

- **能力查询**: "当前还有多少可用槽位？" — 供 Agent 决策时参考
- **强制上限**: 即使 Agent 忽略查询结果，平台在 `start_task` 时也要拒绝超限

## 核心需求

1. 维护 per-project running session 计数
2. 提供查询接口：`available_slots(project_id) -> u32`
3. 在 `TaskLifecycleService.start_task()` 中增加前置检查（硬性拒绝）
4. 可选：暴露为 Project Agent 可调用的 tool（或 context 信息）

## 待讨论

- [ ] 并发限制粒度：仅 per-project？还是也需要 per-story / per-executor-type？
- [ ] 现有 `TaskLockMap` 是 per-task 互斥锁，与全局并发治理如何共存？
- [ ] 是否需要 slot reservation（claim 机制）防止竞态？

## 依赖

- symphony-orchestrator-config（需要 `max_concurrent_sessions` 配置）

## 参考

- Symphony spec §8.3 (Concurrency Control)
- `crates/agentdash-application/src/task/service.rs` — `TaskLifecycleService`
