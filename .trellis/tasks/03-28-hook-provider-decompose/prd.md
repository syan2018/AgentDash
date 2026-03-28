# 拆解 AppExecutionHookProvider 为独立子服务

## Goal

将 `crates/agentdash-application/src/hooks/mod.rs` 中的 `AppExecutionHookProvider` 从一个承担 4 项职责、持有 7 个 repo 依赖的"God Service"，拆解为 3 个职责清晰的独立 struct，`AppExecutionHookProvider` 本身退化为组合者（Facade）。

## 背景

当前 `AppExecutionHookProvider` 同时负责：
1. **Owner 反向解析** — 根据 session binding 查询 project/story/task 信息
2. **Workflow 投影构建** — 调用 `resolve_active_workflow_projection` 组装 workflow/lifecycle 快照
3. **Hook 规则评估** — 在 `rules.rs` 中根据 trigger/policy 产出 HookContributionSet
4. **Step 推进** — 完成 lifecycle step、记录 artifact

这导致：
- 构造函数需要 7 个 `Arc<dyn XxxRepository>` 参数
- 测试时 mock 负担重
- 新增任何一项职责都会让所有其他部分受到影响

## Requirements

### 拆出 3 个独立 struct

| Struct | 职责 | 依赖的 Repo |
|--------|------|------------|
| `SessionOwnerResolver` | 根据 `SessionBinding` 反查 project / story / task 实体，返回 `HookOwnerSummary` | `SessionBindingRepository`, `ProjectRepository`, `StoryRepository`, `TaskRepository` |
| `WorkflowSnapshotBuilder` | 根据 owner 信息构建 `ActiveWorkflowProjection`，填充 metadata | `WorkflowDefinitionRepository`, `LifecycleDefinitionRepository`, `LifecycleRunRepository` |
| `HookRuleEngine` | 纯函数式的 hook 规则评估，不持有任何 repo 依赖 | 无（纯逻辑，接收数据参数） |

### AppExecutionHookProvider 变为 Facade

```rust
pub struct AppExecutionHookProvider {
    owner_resolver: SessionOwnerResolver,
    workflow_builder: WorkflowSnapshotBuilder,
    // HookRuleEngine 为无状态函数集合，不需要持有
}
```

- `impl ExecutionHookProvider for AppExecutionHookProvider` 保持不变，内部委托给子服务
- 现有 `rules.rs` 和 `snapshot_helpers.rs` 子模块继续保留，但公共函数签名改为接收子服务提供的中间结果

### 不变项

- `ExecutionHookProvider` trait 定义（在 `agentdash-connector-contract`）不做任何修改
- 所有现有 test 保持通过（可调整构造方式，不改断言逻辑）
- `agentdash-api/src/app_state.rs` 中的组装代码仅改为分步构造后组合

## Acceptance Criteria

- [ ] `SessionOwnerResolver` 可独立构造和测试
- [ ] `WorkflowSnapshotBuilder` 可独立构造和测试
- [ ] `HookRuleEngine` 中的核心评估函数签名不依赖任何 repo trait
- [ ] `AppExecutionHookProvider` 的 public API（`ExecutionHookProvider` trait methods）保持签名不变
- [ ] 所有现有 `hooks/` 目录下的 unit test 通过
- [ ] `cargo check --workspace` 无错误

## Technical Notes

- 建议执行顺序：先提取 `SessionOwnerResolver` → 再提取 `WorkflowSnapshotBuilder` → 最后整理 `HookRuleEngine`
- `snapshot_helpers.rs` 中的 `resolve_snapshot_metadata_*` 系列函数大多是 `WorkflowSnapshotBuilder` 的内部逻辑
- `rules.rs` 中的函数已经是较纯粹的规则评估，主要需要调整入参来源
- 此 task 与 `hook-snapshot-strong-typing` 有先后依赖：建议先完成本 task 的拆分，再做 metadata 强类型化

## 优先级

P0 — 本系列最高优先级
