# WI-00 Decision Inventory

## Objective

把 research 中的证据转成可执行清单：仓储、表、port、route、DTO、frontend state 都必须归入正式决策分类，作为后续实施的输入。

## Planning Result

已完成。结果见 `../inventory.md`，其中 Q-001 到 Q-008 已从开放问题回填为执行结论。

## Decisions

D-001, D-002, D-003, D-016, D-017, D-018, D-019

## Research Inputs

- `references/adversarial-first-principles-review.md`
- `research/aggregate-ownership.md`
- `research/database-physical-design.md`
- `research/projection-permission-api-frontend.md`
- `research/runtime-session-internal-model.md`

## Scope

- 建立 repository/table/port inventory。
- 标注每个条目的分类：independent fact source、parent-owned child fact、parent-owned child table、application port、runtime trace store、projection/cache。
- 标注疑似冗余物理表：重复事实、重复 projection、错误 owner、无独立查询入口、无锁/扫描/恢复需求。
- 标注当前使用点：domain、application、infrastructure、api、contracts、frontend、tests。
- 对 Conditional 决策列出必须验证的代码事实。

## Out Of Scope

- 不修改生产代码。
- 不做 schema migration。

## Dependencies

无。该工作项是后续实施的入口。

## Deliverables

- 在本任务下新增或更新一个 inventory 文档。
- 更新 `decisions.md` 中 Conditional 条目的状态或验证说明。
- 给 WI-01 到 WI-12 补充明确的输入清单。

## Acceptance

- 每个候选仓储都有分类结论。
- 每个候选物理表都有保留、删除、合并或降级初判。
- 每个分类结论都能引用至少一个代码使用点或 research 证据。
- 每个后续工作项都能从 inventory 中找到自己的改动范围。
- 没有“因为已有 repository 所以保留 repository”的结论。
- 没有“因为已有 table 所以保留 table”的结论。

## Validation

- 使用 `rg` 清点关键符号和 route/DTO/frontend state。
- 对 migration 相关表建立 FK/cascade 初始清单，交给 WI-12 维护。
