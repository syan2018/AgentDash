# 通用启动对账管线

## 目标

服务重启后，通过一条统一的 startup-reconcile 管线确保系统从一致状态恢复。
这不是仅针对 task 的增量补丁，而是一个**通用的、有序编排的启动对账框架**——
将 session、task、agent trigger 等所有需要 boot-time 收敛的业务统一编排。

### 关键设计约束

**对账顺序有依赖**：Session 对账必须先于 Task 对账执行。
Task 的运行/终态判断依赖其关联 session 的状态，如果 session 尚未被清理就先跑 task 对账，
可能误判 "session 仍在运行" 而跳过本应回退的 task。

## 管线阶段（建议执行顺序）

```
Phase 1: Session Reconcile
  - 扫描所有标记为 running 的 session
  - 对已无活跃进程的 session → 标记为 interrupted/terminated
  - 清理 session 关联的内存态资源

Phase 2: Task Reconcile
  - 扫描 Running/Assigned 状态的 task
  - Running task 关联的 session 已终态 → 按策略回退 (Pending / Failed)
  - Assigned 但无 session 的 task → 清理为 Pending

Phase 3: Infrastructure Restore
  - 并发槽位计数重建（从持久化的 running session 数恢复，或内存态归零）
  - 定时触发器重建（若 Project Agent 配置了定时触发）
```

## 与现有 TaskStateReconciler 的关系

当前 `TaskStateReconciler` 仅处理 task 维度的 boot-time 对账：
- Running task + session 已完成 → AwaitingVerification
- Running task + session 已失败 → 根据 ExecutionMode 重试或 Failed

需要将其纳入管线 Phase 2，并增强：
- Running task + session 不存在（进程重启） → 回退到可重新调度状态
- Assigned 无 session 的 task → 清理为 Pending

Session 自身的对账（Phase 1）当前不存在，需要新增。

## 待讨论

- [ ] 管线框架设计：trait-based 可插拔阶段？还是有序函数调用链？
- [ ] 重启后 Running task 回退策略：Pending（可重新调度）vs Failed（需人工介入）？
- [ ] 是否需要通知 Project Agent "刚发生了重启，以下状态已被对账修正"？
- [ ] Session 对账粒度：仅清理标记，还是也包括释放 session 持有的资源（如 address space mount）？

## 依赖

- symphony-concurrency-governor（Phase 3 槽位重建依赖其接口）

## 参考

- Symphony spec §8.6 (Startup Terminal Workspace Cleanup)
- Symphony spec §14.3 (Partial State Recovery)
- `crates/agentdash-application/src/task/state_reconciler.rs`
- `crates/agentdash-application/src/session/hub.rs`
