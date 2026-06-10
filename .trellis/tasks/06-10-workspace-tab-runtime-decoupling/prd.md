# 架构：workspace tab runtime 解耦

## Goal

拆分 workspace tab layout store 与 React render registry，消除 import-order/global singleton 耦合。

## Requirements

- workspace tab store 只保存可序列化的 tab layout / instance 状态。
- React render descriptor registry 不应由 store 直接 import，也不应靠模块顶层副作用决定可用 tab 类型。
- `WorkspacePanel` 或 workspace runtime composition root 应负责注入或注册 render registry。
- 设计时需考虑测试隔离、多 workspace/session 场景和内置 tab 注册顺序。

## Acceptance Criteria

- [ ] `workspaceTabStore` 不再依赖 `features/workspace-panel/tab-type-registry`。
- [ ] tab descriptor contract 与 render registry 边界清晰，store 不持有 React render 函数。
- [ ] WorkspacePanel 能显式组合 layout state 和 render registry。
- [ ] 相关前端 typecheck 和核心 UI 测试通过，测试不依赖全局 registry 残留状态。

## Notes

- 这是结构性前端任务，当前只作为 tracking task；不要在补齐设计前 start。
