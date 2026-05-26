# Findings

## 结论摘要

当前实现的总体方向是 solid 的：compaction 已经被建模为可持久化、可 materialize 的 model context projection；session fork 也已经通过 child session + lineage edge + initial projection 形成独立恢复路径。这比 “直接裁剪 message array” 或 “复制父 session 全量事件” 稳得多。

但现在还不能说足够稳妥易扩展。主要风险不是缺表或缺 UI，而是几个核心语义边界还太松：Codex 原生 compaction 没有进入 projection 链路；projection commit 的 invariant 只在读取时部分校验；fork endpoint 可以写入非 fork relation；fork point 可落在任意 event seq；`branch_id` 与 `session_lineage` 两套 branch 语言并存但没有明确契约。

下面按优先级列出 findings。P1/P2 建议在继续堆功能前先收紧，因为这些属于预研期最应该一次性校正的模型问题。

## Findings

### P1. Codex 原生 compaction 不会写入 model context projection

证据：`crates/agentdash-executor/src/connectors/codex_bridge.rs` 把 `thread/compacted` 映射为 `BackboneEvent::ContextCompacted`；`packages/app-web/src/generated/backbone-protocol.ts` 的 `ContextCompactedNotification` 只有 `threadId/turnId`；`crates/agentdash-application/src/session/eventing.rs` 的 `maybe_commit_compaction_projection` 只消费 platform `SessionMetaUpdate { key: "context_compacted" }` 且要求非空 `summary`。

影响：接入真实 Codex executor 时，UI 可能收到 `context_compacted` 刷新信号，但 `session_compactions`、`session_projection_segments`、`session_projection_heads` 不会更新。后续 projection panel、fork child initial context、resume context 都可能和 Codex 实际 compacted history 分叉。

建议：为 Codex bridge 增加完整 projection commit 材料，优先从 Codex rollout `CompactedItem.replacement_history` 或等价 checkpoint 转成项目内 `context_compacted` platform event；如果拿不到 summary/boundary，就把该事件明确标记为 telemetry，不触发 projection refresh 语义。

### P1. Projection commit 边界校验太弱

证据：SQLite/Postgres 的 `validate_commit_session` 只校验 compaction/head/segments 的 `session_id` 是否一致。它不校验 `branch_id`、`projection_kind`、`projection_version`、`head.active_compaction_id == compaction.id`、segment `generated_by_compaction_id`、segment version/kind 是否与 compaction/head 一致。`compaction_checkpoint.rs` 在 materialize 时会做部分校验，但那已经是读路径。

影响：坏 commit 可以成功写入 head，之后所有依赖该 head 的 context build 才失败。对 compaction/fork/rollback 这类基础能力来说，错误应在 commit 边界被拒绝，而不是等用户打开会话或 agent 恢复时暴露。

建议：把 checkpoint invariant 前移到 `commit_compaction_projection` 的 shared validation 中，并尽量补 DB 约束或唯一性约束。预研期无需兼容坏数据，应该让 commit fail fast。

### P1. `POST /sessions/{id}/fork` 的 relation kind 过宽

证据：`crates/agentdash-contracts/src/session.rs` 的 `CreateSessionForkRequest` 暴露 `relation_kind`；`crates/agentdash-api/src/routes/acp_sessions.rs` 的 `relation_kind_from_dto` 接受 `fork`、`companion`、`spawned_agent`、`rollback_branch`，默认才是 `fork`。但 `SessionBranchingService::fork_session` 的实现始终是 child session + parent projected context + fork initial projection。

影响：普通 fork endpoint 可以写出 companion/spawned_agent/rollback_branch edge，但这些 relation 的权限、runtime、agent transcript、rollback 语义并没有在该 use case 中实现。长期会污染 lineage 查询和 UI 语义。

建议：普通 fork endpoint 固定创建 `fork` relation。`companion`、`spawned_agent`、`rollback_branch` 由专门 service 创建，并在各自入口表达 runtime/binding/context 差异。

### P2. Fork point 可落在任意 event seq，缺少 turn/message boundary 约束

证据：`resolve_fork_point` 优先接受 `fork_point_event_seq`，只校验它不超过当前模型可见 head；没有确认该 event seq 是否对应稳定 user message、turn boundary、tool call/result 完整边界。对照 Codex，fork/truncate 会显式处理 user message boundary 和未完成 active turn。

影响：child initial projection 可能 materialize 半个 assistant turn、半组 tool interaction 或不可继续的上下文。当前 context projector 能按 event seq 截 raw suffix，但业务语义不一定完整。

建议：把 fork point 收敛到 `MessageRef` 或明确 turn boundary。裸 event seq 只用于内部调试或必须校验成合法 projected entry；active/running turn 下禁止 fork，或定义 mid-turn fork 的完整截断规则。

### P2. Compaction boundary 依赖事后推导，失败时静默降级

证据：runtime `AgentMessage::compaction_summary` 默认 `compacted_until_ref: None`；`eventing.rs` 的 `maybe_commit_compaction_projection` 会 `list_all_events`，从 `messages_compacted` 和历史事件推导 boundary/source range。如果无法得到 `source_end_event_seq`，函数直接返回 `Ok(None)`，事件按普通 event append，不写 projection commit。

影响：agent 内存里已经采用 compacted messages，持久化 projection 却可能没有 checkpoint；resume/fork/面板会和实际 provider context 分叉。长 session 下每次 compaction commit 还会付出全量事件扫描成本。

建议：在 `AgentEvent::ContextCompacted` 或 stream mapper 中携带明确 `compacted_until_ref/source_end_event_seq/first_kept_event_seq`，commit 无法解析时写 diagnostics 并显式失败，不要静默当普通事件处理。后续再考虑增量 projection cache，替代每次全量扫描。

### P2. `branch_id` 与跨 session lineage 的概念需要固定

证据：`0059_session_compaction_projection_store.sql` 给 compaction/segments/heads 都建了 `branch_id`，但 live compaction、fork initial projection、rollback 都传 `None`。真正的 session branching 通过 `0060_session_lineage.sql` 的 parent-child edge 表达。

影响：代码里同时存在 “projection branch_id” 和 “session branching/lineage”。如果后续直接把同 session branch tree、rollback branch、child session fork 混用，API、UI、projection head 查询都会变得难以推理。

建议：短期文档化：`branch_id` 是 projection namespace 预留，当前产品 branch 是跨 session lineage。中期决定是否引入同 session branch tree；如果引入，应补完整 leaf/head/branch summary 模型，而不是只开始传非空 `branch_id`。

### P2. Fork 与 binding copy 不是严格原子流程

证据：`fork_session` 依次 create child、upsert lineage、commit child projection，失败时 best-effort delete child；API 层复制 parent bindings 失败时也 best-effort delete child。

影响：跨 repository 失败会有短暂不一致窗口；如果 delete 失败，可能留下没有 bindings 或没有完整 projection 的 child session。预研阶段可接受，但如果 fork 进入主流程，这会变成可见稳定性问题。

建议：把 fork 创建建模为 pending -> committed 状态，或在同一事务/同一 application unit-of-work 中完成 session、lineage、projection、bindings。至少让 cleanup failure 有 diagnostics 和后台修复入口。

### P2. Lineage UI 仍是诊断面板，不是完整分支体验

证据：`SessionLineageView` 只展示 parent summary、计数和 direct children；项目列表的 `active-session-list.tsx` 对 relation `linkedChildren` 只渲染一层 `SessionRow`，不递归展示 fork 的 fork。前端服务层已有 `forkSession`、`rollbackSessionProjection`，但聊天页没有对应操作入口。

影响：用户能看到“有分支”，但无法把 branching 当作日常工作流使用：不能从 lineage 面板跳 parent/child，不能看到完整 tree，不能从 UI 创建 fork/rollback，也看不到 branch summary/handoff。

建议：把当前面板定位为 observability；产品化时补完整 tree、跳转、fork/rollback action、nested relation children，以及每个 branch 的 projection/compaction 状态。

### P3. Token 估算和自动触发策略仍偏粗

证据：compaction cut point 使用 chars/4 的估算，hook preset 用 `last_input_tokens > context_window - reserve`。这比没有策略好，但对中文、JSON/tool payload、图片 data、provider-specific tokenizer 都会有误差。

影响：可能过早或过晚 compact，tail 保留长度也会波动。对预研可接受，对高可靠运行还不够。

建议：保留当前保守默认，同时逐步引入 provider tokenizer 或 provider usage 反馈；至少在 diagnostics 中记录估算来源和误差。

## 建议路线

### Phase 1：收紧契约

- 修复 Codex `thread/compacted` 与 projection commit 的语义断点。
- 加强 `commit_compaction_projection` invariant validation。
- 限制 fork endpoint 只能创建 `fork` relation。
- 明确文档化 `branch_id` 与 `session_lineage` 的分层。

### Phase 2：稳定 boundary 与恢复

- 让 runtime compaction event 携带 source boundary / first kept pointer。
- fork point 收敛到 message/turn boundary。
- compaction commit 无法 materialize 时显式失败并写 diagnostics。
- 为 long session 设计增量 projection/head 读取，减少全量 event scan。

### Phase 3：产品化 branching

- 做完整 lineage tree UI、parent/child 跳转、fork/rollback 操作入口。
- 支持 nested relation children。
- 决定是否需要 branch summary/handoff；如果需要，参考 pi-mono 的 branch summary，而不是复用 compaction summary。

### Phase 4：高级扩展

- 如果要做同 session branch tree，再正式启用 `branch_id`，并引入 leaf/head/branch summary 的完整模型。
- 如果要做可恢复 spawned agent，把 `spawned_agent` relation 与 agent transcript/runtime session 绑定起来。
- 如果要做多 projection kind，按 `timeline/audit/handoff/model_context` 分别定义 materialization 和 head 语义。
