# Contract Boundary 分层审计

## Goal

审计 application、contracts、API adapter 与 frontend generated contracts 的 DTO owner 和转换边界，形成可迁移的 owner map，避免 use case/read model 与 browser-facing wire DTO 同步演进。

## Scope

- `agentdash-application` 直接依赖 `agentdash-contracts` 的 import-level audit。
- `agentdash-contracts` 内部 domain/SPI/protocol conversion 边界。
- application read model、API adapter、contract DTO owner map。
- 高风险入口的后续迁移任务拆分。

## Out Of Scope

- 已完成的机械 contract 化任务不重复创建。
- 不做全量迁移；先审计并标注 owner，再挑高风险入口实现。

## Acceptance Criteria

- [ ] `design.md` 定义允许/不允许的 conversion 方向。
- [ ] `work-items/index.md` 覆盖 D01 和 contracts crate 内部转换边界。
- [ ] import-level audit 产物列出每个 application -> contracts import 的归属判断。
- [ ] 后续实现任务能按 owner map 分批迁移。

