# SessionContextSnapshot Mount 化

## 背景

当前 `SessionContextSnapshot`（含 executor、project_defaults、effective、owner_context）通过 `build_*_session_context()` 构建后，作为 API response 字段一次性返回。这些信息对 agent 在 session 过程中理解自身配置和上下文有价值，但目前不在 mount 体系内，agent 无法通过 address space 工具按需读取。

来源：context-binding-simplification PRD 的 P4a 讨论项。

## 待决策

- SessionContextSnapshot 是否需要 mount 化？还是 bootstrap 注入已够用？
- 如果 mount 化，provider 设计（`session_context_vfs`）和路径结构如何定义？
- 与 lifecycle mount 的关系（是否合并为一个更通用的 "session metadata" mount）？

## 状态

Parking — 等 context-binding-simplification 完成后再评估。
