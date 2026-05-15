# Technical Design：Batch 1 Owner + Construction

## Target Shape

```text
SessionOwnerResolver
  -> ResolvedSessionOwner
  -> SessionConstructionPlan
  -> context projection
  -> legacy PromptSessionRequest boundary
```

Batch 1 不新增 launch service。`SessionConstructionPlan` 是共享事实源，不是旧 request 的改名；它必须至少被 context endpoint 使用，并为后续 Batch 2 的 `LaunchExecution` 提供稳定输入。

## Ownership Boundary

`session/ownership` 负责：

- 输入 session binding candidates 与 optional launch owner hint。
- 输出单一 `ResolvedSessionOwner`。
- 暴露明确 priority：Task -> Story -> Project，作为 launch/query/展示的统一规则。
- 提供 trace：候选 owner、选中 owner、选中原因。

旧的 context query `Project -> Story -> Task` 逻辑在本批被删除或替换。Batch 0 characterization 测试应改成目标态断言。

## Construction Boundary

`session/construction` 负责：

- 汇聚 owner、workspace、context bundle、VFS、MCP、capability、identity、execution profile 与 working dir trace。
- 输出 `SessionConstructionPlan`，保持长期 session fact 和 projection 可查询。
- 提供 `SessionContextProjection`，供 API route 生成 response。

初始实现可以复用 `SessionAssemblyBuilder` / `PreparedSessionInputs` 的 composer 逻辑，但不能把 `PromptSessionRequest` 当作 construction plan 的唯一事实源。

## Route Boundary

API route 负责：

- 鉴权、参数解析、调用 application 层 use case。
- 将 projection 映射为 response DTO。

API route 不再负责：

- 自己排序 owner bindings。
- 自己根据 Project/Story/Task 分支拼 context。
- 自己重建 VFS/MCP/capability 主线。

## Migration Notes

- `PromptSessionRequest` 在 Batch 1 仍可作为旧 pipeline 输入，但只能从 construction/adapters 的结果投影，不能继续扩散为新的事实源。
- `finalize_augmented_request` 如果暂时无法删除，需要明确缩到单一迁移调用点，并在 Batch 2/3 删除。
- 任何新增中间类型都必须承担事实或投影职责；只传递同一组字段的中转层不进入本批。
