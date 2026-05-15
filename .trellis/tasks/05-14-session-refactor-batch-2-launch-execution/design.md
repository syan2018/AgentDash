# Technical Design：Batch 2 LaunchExecution

## Target Shape

```text
legacy PromptSessionRequest + SessionConstructionPlan/owner fact + runtime facts
  -> LaunchExecution
  -> ExecutionContext projection
  -> connector.prompt
```

Batch 2 的核心是减少 `start_prompt_with_follow_up` 内部的状态分支，而不是急着替换所有入口。旧 request 仍是 adapter 边界，但执行策略必须前置成 plan。

## LaunchExecution Fields

最小字段：

- `session_id`
- `turn_id`
- `payload`
- `lifecycle`
- `hook_snapshot_reload`
- `pending_capability_state_transitions`
- `connector_context`
- `launch_summary`

`connector_context` 可以暂时持有 `ExecutionContext`，但 design 上它是 connector projection，不是长期事实源。

## Migration Boundary

本批优先从 `prompt_pipeline.rs` 中抽出纯解析函数：

- pending transition extraction
- effective VFS/MCP/capability/working_dir projection
- hook reload trigger selection
- lifecycle/restore summary

副作用仍留在 pipeline：

- turn reservation
- event append
- connector setup/prompt
- processor spawn
- meta persistence

## Safety

- 先保持现有行为，再通过 plan 收口变量来源。
- pending transition 仍沿用 `SessionMeta` 字段，事件化留到 Batch 6。
- connector failure 的 terminal behavior 必须保持 Batch 0 测试。
