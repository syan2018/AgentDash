# 会话分支与状态投影管理

## Goal

在上下文压缩 projection 基建稳定后，为 AgentDash 平台自有 Agent runtime 建立完整的 session branch / fork / rollback 能力。新系统需要把会话树、模型可见投影、UI 审计历史和业务 owner binding 分开建模，使用户可以从稳定分叉点创建新会话分支、回退模型可见状态，并在前端理解当前会话与其它分支的关系。

## Parent And Dependency

本任务是 `.trellis/tasks/05-25-context-compaction-architecture-enhancement` 的后续子任务。

父任务已交付以下最小基建，本任务在此之上推进：

- `session_compactions`：可查询的模型恢复 checkpoint 表面，含 lifecycle、source range、`first_kept_event_seq`、token stats、`replacement_projection_json`。
- `session_projection_segments`：表达 summary chunk、kept tail、pruned source、artifact reference 等模型可见片段。
- `session_projection_heads`：表达当前模型可见 active cursor，rollback 后 restore 不会越过 active head。
- `ContextProjector`：以 `projection head -> active compaction -> segments -> suffix events` 构建模型输入。

本任务不重新设计压缩策略；它消费父任务提供的 compaction-as-checkpoint 和 projection 契约，并补齐 session tree 所需的 durable lineage 与投影移动能力。

## User Value

- 用户可以从某个历史状态继续探索替代路径，而不需要复制整段对话或从头重开。
- 用户可以把模型可见状态回退到某个明确边界，同时保留完整审计历史和 UI feed。
- 分支关系、companion 子会话和业务 owner binding 不再混在一个字段里，后续能稳定支持 tree UI、branch 列表、fork 来源展示和分支状态管理。
- 长会话压缩后的 fork / rollback 能基于 checkpoint 恢复，而不是重新 replay 全量历史或复制大段事件。

## Confirmed Facts

- AgentDash 已有 `sessions`、`session_events`、`SessionMeta.last_event_seq` 和 session repository abstraction。
- 当前前端/接口里已有 `parent_session_id` 概念，但主要来自 `companion_context`，它表达 companion 父子关系，不足以承载通用 session branch lineage。
- `MessageRef { turn_id, entry_index }` 和 `ProjectedTranscript` 已存在，用于 compaction cut boundary、restore 对齐和 branch lineage。
- Codex 的 branch/fork/rollback 以 rollout JSONL replay 为核心；AgentDash 采用数据库仓储，更适合用 event log、checkpoint table、lineage table 和 projection head 表达同类语义。
- 父任务决定 Codex Bridge 保留自身内部压缩和 session state；本任务只管理平台自有 Agent runtime 的 session branch。

## Requirements

### R1. 分支拓扑必须进入独立 lineage 模型

- `session_lineage` 必须表达 `parent_session_id`、`child_session_id`、`relation_kind`、`fork_point_event_seq`、`fork_point_ref`、`fork_point_compaction_id`、`status`。
- `relation_kind` 至少覆盖 `fork`、`companion`、`spawned_agent`，并预留 rollback branch / future branch 类型。
- 同一个 child session 只能有一个 primary parent，便于 tree UI 和恢复基线推导。
- `session_lineage` 不替代 `session_bindings`；业务归属仍由 project/story/task owner binding 表达。

### R2. fork 必须以稳定 projection 为基线

- fork 创建 child session 时必须记录 fork point。
- fork point 可以是 `event_seq`、`MessageRef`、或 compaction id；实现必须能恢复出 fork 时的 parent projection。
- 默认在 fork 时 materialize child initial compaction checkpoint，把 parent fork projection 固化到 child session，换取 child 独立恢复能力。
- fork 后 parent 的继续压缩、rollback、archive 不应改变 child 的初始可见状态。

### R3. rollback 必须保留审计历史

- rollback 不删除 `session_events`。
- rollback 写入结构化 platform event，并更新 active projection cursor。
- 模型 restore、continuation、executor restore 使用 active projection cursor 判断当前模型可见历史。
- UI feed 可以展示 rollback 发生过，但不会把 rollback 后的旧 checkpoint 当成 active restore source。

### R4. branch-aware restore 必须可测试

- 从 root session fork 出 child 后，child restore 使用 fork point 之前的 projection 和 child suffix。
- parent fork 后继续产生事件，不影响 child restore。
- rollback 到 checkpoint 之前后，后续 checkpoint 不再被选为 active checkpoint。
- 多级分支按 lineage 查询能够稳定返回 parent/child 路径。

### R5. 前端展示要从仓储契约消费数据

- 前端 session list / tree 不从 `companion_context.parent_session_id` 推断通用 branch。
- API 返回明确 lineage relation、status、fork point 和 branch display metadata。
- 初版 UI 可以先做 branch list / parent-child grouping；完整树形可视化可作为后续增强。

## Acceptance Criteria

- [ ] 新增或扩展 repository 能查询 session lineage 的 direct children、ancestors、descendants，并保持稳定排序。
- [ ] fork API 能创建 child session、记录 lineage edge、固化 fork projection 为 child initial compaction，并返回 child session meta。
- [ ] rollback API 能追加 rollback event、更新 active projection cursor，并让 restore 使用 rollback 后的模型可见 head。
- [ ] continuation / executor restore 在 branch 和 rollback 场景下通过测试。
- [ ] API / 前端消费 `session_lineage`，不把 companion parent 误当作通用 branch。
- [ ] PostgreSQL / SQLite migration 同步，相关 Rust repository tests 通过。
- [ ] 前端至少覆盖 branch 列表或 parent-child grouping 的单元测试。

## Out Of Scope

- 不改变父任务的压缩策略选择、token budget cut 或 summarizer 实现。
- 不接管 Codex Bridge 内部 session tree / rollout / compaction 逻辑。
- 不做旧 JSON tree 形态兼容。
- 不在第一阶段实现复杂图形化 branch canvas；初版以可查询、可恢复、可展示为目标。

## Open Question

- fork 时 materialize child initial checkpoint 的默认策略已确认：写入更重，但 child session 可以脱离 parent retention 独立恢复，也能让后续团队协作、审计和 rollback 都只依赖 child 自己的 projection head。
