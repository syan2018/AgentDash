# Workflow 动态上下文与模板化 Locator

## Goal
把 workflow 所需的动态上下文从“代码里硬编码的语义型 context key”迁移为真正的 lifecycle VFS 资源路径，并支持受限模板变量展开。

## Requirements
- lifecycle runtime 可以把上下文物化到统一的 VFS 目录，优先考虑 `lifecycle://.../context/...` 路径族。
- `WorkflowContextBinding.locator` 应引用真实的 VFS 路径，而不是 `execution_context` / `review_checklist` 这类语义型硬编码 key。
- 支持受限模板变量展开，用于在 locator 中拼接稳定运行时变量，例如 step key / run id / binding id。
- resolver 仍保持通用职责，只做模板展开、VFS 路径解析和读取，不理解 checklist / journal / archive 等业务语义。

## Acceptance Criteria
- [ ] 设计一套统一的 lifecycle context 路径约定，并明确 run 级 / node 级上下文的边界。
- [ ] 明确第一版允许的模板变量集合、缺失变量报错语义和禁用能力范围。
- [ ] builtin workflow 示例改为引用真实 lifecycle VFS 路径，不再依赖语义型 context key。
- [ ] 输出迁移方案，说明现有 hook / completion / artifact 逻辑如何与动态上下文目录衔接。

## Notes
- 本次 task 只用于后续迭代跟踪；当前优先目标是把已经引入的硬编码 context 清理干净。
