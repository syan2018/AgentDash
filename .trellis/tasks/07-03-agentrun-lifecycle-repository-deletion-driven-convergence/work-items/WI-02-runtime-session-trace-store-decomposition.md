# WI-02 RuntimeSession Trace Store Decomposition

## Objective

把 RuntimeSession 持久化边界收窄为内部 trace store，删除 application service 对 `SessionPersistence` mega trait 的依赖。

## Decisions

D-003, D-004, D-018

## Research Inputs

- `research/runtime-session-internal-model.md`
- `research/database-physical-design.md`

## Scope

- 将 `SessionPersistence` 拆成窄 port：event log、meta、projection、terminal effects、runtime commands、lineage、compaction。
- 删除 `SessionPersistenceStoreAdapter` 或等价 mega adapter 的应用层注入。
- 让 runtime-session service 构造函数只接收自身需要的 store。
- 评估 `sessions` / `session_*` 是否在本轮直接迁移为 `runtime_sessions` / `runtime_session_*`。

## Out Of Scope

- 不迁移产品 route；交给 WI-01。
- 不处理 AgentRun command queue；交给 WI-04。
- 不处理 RepositorySet 总体 cleanup；交给 WI-11。

## Dependencies

依赖 WI-00 的 store 使用点清单。schema 命名变更需要交给 WI-12 排期。

## Implementation Notes

- `session_events` 已接近 envelope-only event log，保持 append-only event fact。
- runtime commands 若保留，命名应表达 delivery/runtime 内部操作，不表达用户 command。
- meta update 应改为 field-specific methods，避免任意 closure 更新把 RuntimeSession 再次变成外部可写 aggregate。

## Acceptance

- application service 不再能通过一个 trait 触达所有 runtime session stores。
- 每个 RuntimeSession service 的依赖能从构造函数看出。
- runtime trace store 不暴露产品权限、产品 fork、产品 command 语义。
- 若决定重命名表，WI-12 已登记 migration 和 FK/cascade 影响。

## Validation

- `rg "SessionPersistence|from_persistence|SessionPersistenceStoreAdapter"` 无业务层残留。
- Rust 编译和 runtime-session 相关测试通过。
- store trait mock / memory implementation 与 Postgres implementation 同步。
