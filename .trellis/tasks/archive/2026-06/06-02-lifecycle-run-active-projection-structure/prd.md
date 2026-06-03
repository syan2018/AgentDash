# LifecycleRun 活跃 Activity 投影结构化

## Goal

把 `LifecycleRun.active_node_keys` 从业务可读写事实源降级或移除，改由 `WorkflowGraphInstance.activity_state` 派生结构化 `ActiveActivityRef`。当前代码已经定义 `ActiveActivityRef`，但它仍从 `active_node_keys` 字符串解析而来；数据库和 repository 仍持久化 `active_node_keys`，部分工具和 read model 仍直接使用字符串集合。本任务只处理这条剩余收口线。

目标状态：Activity runtime state 的事实源是 `WorkflowGraphInstance.activity_state`；`LifecycleRunView` 暴露 structured active refs；后端业务路径不依赖 `graph_instance_id:activity_key` 字符串协议。

## Current Baseline

- `ActiveActivityRef` 已在 domain 中存在。
- `LifecycleRun::active_activity_refs()` 当前从 `active_node_keys` 解析。
- `lifecycle_runs.active_node_keys` 仍在 baseline migration、repository、domain entity 中持久化。
- `advance_node`、journey projection 和部分 tests 仍读取或输出 `active_node_keys`。
- 前端主要通过 generated lifecycle/runtime views 消费运行态，不需要保留字符串 active key 协议。

## Requirements

- `ActiveActivityRef` 至少包含 `run_id`、`graph_instance_id`、`activity_key`、`attempt`、`status`。
- `LifecycleRunView` / generated TS 暴露 structured active refs。
- active refs 应从 `WorkflowGraphInstance.activity_state` 派生；除非明确有同步 owner，否则不要新增第二套 persisted active refs。
- `active_node_keys` 若短期保留，只能作为 debug display，不得作为业务推进、completion 或 route response 的事实源。
- `current_activity_key()` 不得作为业务路径事实源；调用方应使用 assignment / graph instance / activity attempt identity。
- `select_active_run`、`advance_node`、journey projection 等入口不得依赖字符串 split/first-active 语义。
- migration / contracts / generated TS 直接进入目标形态，不保留旧字段兼容。

## Acceptance Criteria

- [ ] `LifecycleRunView` 暴露 structured `active_activity_refs`。
- [ ] `active_node_keys` 从 runtime fact source 退场；生产路径不再拼接或解析 `graph_instance_id:activity_key`。
- [ ] 后端业务入口不再通过 `current_activity_key()` 或 `active_node_keys.first()` 推进 Activity。
- [ ] `advance_node` 输出 structured active refs，不输出字符串事实源。
- [ ] generated TS 与前端消费点使用 structured refs。
- [ ] 多 graph instance 中同名 activity key 能产生两个独立 active refs。
- [ ] 相关 spec 更新为 `WorkflowGraphInstance.activity_state` 派生 active projection 的目标语义。

## Out Of Scope

- 不重新设计 WorkflowGraph topology。
- 不实现 fork/join 新功能。
- 不处理 scoped artifact 存储；该部分由 sibling task 承担。
