# LifecycleRun 活跃 Activity 投影结构化

## Goal

将 `LifecycleRun.active_node_keys` 这类字符串拼接的 run-level 活跃节点投影收敛为结构化 `ActiveActivityRef`，并确保 runtime 业务路径以 `WorkflowGraphInstance.activity_state` 为事实源。

## User Value

- run-level active projection 可读、可校验，不再隐藏 `graph_instance_id:activity_key` 字符串协议。
- 前后端 read model 能明确展示 graph instance、activity、attempt。
- 后续多 graph instance、fork/join、routine phase 等场景不会被旧单当前节点语义限制。

## Confirmed Facts

- `LifecycleRun.active_node_keys` 当前由 `sync_graph_instance_activity_projections` 拼接 `graph_instance_id:activity_key`。
- `LifecycleRun.current_activity_key()` 返回第一个 active key。
- `LifecycleRunView` 仍暴露 `lifecycle_id`，而 root graph instance 已成为目标语义。
- `WorkflowGraphInstance.activity_state` 已经持有 graph-scoped attempts。
- 前端业务 UI 当前没有直接消费 `active_node_keys`；后端 runtime/control path 仍有 `select_active_run`、`advance_node` 等位置使用 run-level active projection。

## Requirements

- 定义结构化 `ActiveActivityRef`，至少包含 `graph_instance_id`、`activity_key`、可选 `attempt`、`status`。
- runtime 业务逻辑不应依赖 run-level active projection 推进 Activity。
- `LifecycleRunView` 可暴露 structured active refs 作为 read model。
- `lifecycle_id` 的目标命名应与 root graph / workflow graph instance 语义对齐。
- 字符串 active key 若存在，只能作为调试 display，不作为事实源。

## Acceptance Criteria

- [ ] `active_node_keys` 被替换为或降级为结构化 read projection。
- [ ] `current_activity_key()` 不再作为业务路径事实源。
- [ ] `LifecycleRunView` 暴露 structured active refs。
- [ ] 前端使用 structured fields 展示 active Activity。
- [ ] 后端 `select_active_run`、`advance_node` 等入口不再依赖字符串 active key。
- [ ] 测试覆盖多 graph instance 同名 activity key。

## Out Of Scope

- 不重构 WorkflowGraph topology。
- 不实现 fork/join 新功能，仅保证当前投影支持目标语义。

## Dependency Notes

- 可在 runtime anchor 和 scoped artifact 后实施，减少同时修改 Activity identity 的风险。
