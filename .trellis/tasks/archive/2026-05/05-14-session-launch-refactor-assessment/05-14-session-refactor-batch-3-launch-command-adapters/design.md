# Technical Design：Batch 3 Gate

## Intended Target

```text
Source Adapter
  -> LaunchCommand
  -> SessionConstructionPlan
  -> LaunchExecution
  -> connector boundary
```

## Why This Is Gated

Batch 2 已引入 `LaunchExecution` 与 connector projection，但 `prompt_pipeline.rs` 仍保留部分策略 fallback：

- VFS/MCP/capability fallback
- lifecycle restore 解析
- hook runtime reload/refresh
- follow-up session id 解析
- pending transition apply plan

如果现在迁移入口，`LaunchCommand` 只能继续携带这些隐式字段，等价于给 `PromptSessionRequest` 换名。

## Required Pre-Step

创建并完成 Batch 2 follow-up：

- 将剩余纯解析搬入 `LaunchExecution` builder。
- `start_prompt_with_follow_up` 只保留 reservation、event write、connector prompt、processor/supervision。
- connector prompt 前 summary 覆盖 lifecycle、restore、hook、follow-up、pending apply plan。

Batch 3 只有在该 pre-step 完成后才进入代码实现。
