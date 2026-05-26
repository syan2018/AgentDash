# Compaction / Session Branching 语义收束实现说明

## Goal

记录本轮 compaction、projection store 与 session fork/lineage 语义收束的实现范围、约束和验收结果。该任务只作为 PRD-only 归档入口，承接 review 结论到代码落地之间的需求边界，不再拆分额外设计或执行文档。

## Requirements

- 外部 executor 自行完成且缺少 summary、boundary、replacement history 的 compact，只能进入遥测事件，不能影响 AgentDash-owned model context projection。
- 内部 platform compact 必须以显式 `MessageRef` boundary 形成 projection commit 契约，summary、`compacted_until_ref`、`first_kept_ref` 是可恢复模型上下文的必要事实。
- Projection store 的职责收束为保存单个 session 当前 projection kind 的可恢复 head 和 segments；跨 session 分支拓扑由 `session_lineage` 表达。
- 普通 fork endpoint 只表达用户发起的 `Fork` relation。其它 lineage relation kind 保留为读取事实，由各自业务服务负责创建。
- Rollback 的语义是移动当前 projection head 并保留 append-only factual timeline。
- 前端模型上下文刷新只响应内部 projection commit 成功后的事实；外部 compact 遥测可以展示状态，但不刷新 model context panel。
- Contracts、generated TS types、repository schema、SPI records、memory persistence、SQLite/Postgres persistence 与测试 fixture 必须保持同一语义形状。

## Acceptance Criteria

- [x] Codex bridge 将 `thread/compacted` 映射为 `executor_context_compacted`，且该事件不进入 compaction projection commit。
- [x] Pi/native compaction 执行链路保留 message refs，并在 `context_compacted` payload 中提供 `summary`、`compacted_until_ref`、`first_kept_ref`。
- [x] 后端在 summary 或 boundary 缺失时返回错误，不 append 普通 `context_compacted` 事件，不写 projection head/segments/compactions。
- [x] `session_compactions`、`session_projection_segments`、`session_projection_heads` 的 runtime schema 与 records 不再包含 `branch_id`。
- [x] 新增 Postgres migration 删除 projection store `branch_id`，并重建 projection head primary key 与 segment unique key。
- [x] Projection commit validation 覆盖 projection kind/version、active compaction、segment owner 与 source range consistency。
- [x] `CreateSessionForkRequest` 不再接收 `relation_kind` 与 `fork_point_event_seq`；普通 fork 始终写入 `Fork` lineage。
- [x] 前端 generated contracts、stream/feed reducer、projection refresh key 与相关测试已同步。
- [x] 架构 spec 记录外部 compact 遥测、内部 boundary 契约、projection/head 与 lineage 职责分离的原因。

## Notes

- 该 PRD 是实现后的收束索引，代码级细节以当前提交 diff 与 `.trellis/spec/backend/session/`、`.trellis/spec/cross-layer/backbone-protocol.md` 为准。
- review 证据与阶段性分析保存在 `docs/reviews/2026-05-26-compaction-session-branching-review/`。
