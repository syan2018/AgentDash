# 前端规范索引

前端 spec 先读 architecture 主文档，再读取类型、状态、UI、流式协议等附录。

## Architecture Entry

- [Frontend Architecture](./architecture.md)

## Contract And Convention Appendices

| 文档 | 说明 |
| --- | --- |
| [Directory Structure](./directory-structure.md) | package / feature module 当前基线 |
| [Type Safety](./type-safety.md) | DTO 字段、mapper 与运行时验证 |
| [State Management](./state-management.md) | Zustand store 分层与当前 store baseline |
| [Hook Guidelines](./hook-guidelines.md) | NDJSON stream hook 与 feed 聚合契约 |
| [Component Guidelines](./component-guidelines.md) | 组件分层与基础约定 |
| [Design Language](./design-language.md) | token、surface、primitive 当前基线 |
| [Quality Guidelines](./quality-guidelines.md) | 前端编码质量约定 |
| [Activity Lifecycle](./workflow-activity-lifecycle.md) | Activity lifecycle 编辑与运行观察契约 |

## 阅读规则

- 跨后端 DTO、事件流或 runtime surface 的改动，先读 [Cross-layer Architecture](../cross-layer/architecture.md)。
- UI 变更先确认 [Design Language](./design-language.md) 是否已有 primitive 或 token。
- 类型/mapper 变更先读 [Type Safety](./type-safety.md)，避免引入字段别名兼容。

