# Companion Gate Lineage 迁移

## 目标

把 companion wait/adoption/parent-child lineage 从 session metadata 和 in-memory registry 迁到 durable `LifecycleGate`、`LifecycleAgent`、`AgentFrame`、`AgentLineage`。

## 依赖

- 父任务：`06-01-session-lifecycle-control-plane-refactor`
- 依赖：`06-01-session-lifecycle-target-anchors-schema`
- 依赖：`06-01-workflow-agent-assignment-migration`

## 蓝图阶段

- 推进：`target-state-blueprint.md` B5 Business Subject Migration。
- 退出贡献：Companion wait/adoption/lineage 使用 LifecycleGate、LifecycleAgent、AgentFrame、AgentLineage；child run 只用于独立生命周期边界。

## 重构模式

- 采用父任务 `target-state-blueprint.md` 中的 breaking-mode 约束。
- 删除 in-memory wait / SessionMeta companion control truth，不保留 fallback。
- Companion flows 可以在 durable gate 和 agent/frame lineage 全部接好前暂时不可用。

## 需求

- companion wait / resume 写入 `LifecycleGate`，不以 `SessionMeta.companion_context` 作为事实源。
- companion / review / task helper 默认作为 same-run `LifecycleAgent` 与 `WorkflowGraphInstance`；只有独立上下文信道或独立生命周期管理成立时才写 linked/spawned run lineage。
- companion child 以 `LifecycleAgent` 身份存在，并通过 `AgentFrame` 记录 context slice / capability surface。
- parent-child runtime trace 继续保留在 `RuntimeSessionLineage`，但 Agent 关系写入 `AgentLineage`。
- hook pending action 与 companion adoption 使用同一 gate/resume 语义。

## 交付物

- durable `LifecycleGate` wait/resume/adoption path。
- `AgentLineage` parent/child control tree。
- companion child 的 `LifecycleAgent` / `AgentFrame` 建模。
- same-run companion graph 与 linked run 的判定规则。

## 不承担

- 不把 RuntimeSession lineage 当作 ownership。
- 不因为 companion 是子图就默认创建 child `LifecycleRun`。
- 不恢复 `SessionMeta.companion_context` 作为控制面事实源。

## 验收标准

- [ ] companion resume 可以在进程重启后从 durable gate 恢复。
- [ ] companion child 的业务归属可通过 LifecycleSubjectAssociation / AgentLineage 查询。
- [ ] companion graph 不因“是子图”而自动创建 child LifecycleRun。
- [ ] `SessionMeta.companion_context` 不再是 companion 控制面事实源。
- [ ] runtime session lineage 只用于 trace / debug / fork 视图。
