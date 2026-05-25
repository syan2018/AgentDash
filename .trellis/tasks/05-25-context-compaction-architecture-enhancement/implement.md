# 上下文压缩系统重构推进计划

## Intent

本计划把上下文压缩从当前 agent loop 内部的消息数组替换，推进为对齐 Codex app protocol、由 PostgreSQL durable facts 驱动、支持 projection / resume / branch 的上下文基础设施。

推进原则：

- 先锁定协议与事实源，再落 checkpoint / projection store。
- 先让 MVP summary compact 具备可靠恢复能力，再扩展 tool pruning、reactive compact、branch handoff。
- 每个阶段都以可验证的行为收束，避免把未校准的细节直接固化为长期接口。
- 项目仍处预研期，schema 和 API 直接朝正确形态收敛，数据库变更通过 migration 表达。

## Phase 0. 决策锚点校准

目标：把 implement 阶段需要的剩余事实补齐，形成可施工的最小契约。

### 0.1 Codex Protocol Method 对齐

检查点：

- 当前 vendored `codex_app_server_protocol` 中 compact completed notification 的真实 method 名称。
- `ThreadItem::contextCompaction` 是否已经完整进入 generated Backbone TS。
- `CodexBridgeConnector` 应消费的输入集合：`item/started`、`item/completed`、legacy completed marker。

产出：

- 在 implement notes 中记录当前项目实际 method mapping。
- 明确 legacy completed marker 只作为外部 runtime lifecycle 事实，不作为 AgentDash-owned checkpoint 来源。

验收：

- 能指出 Codex Bridge compact 输入事件的完整集合。
- 能解释哪些事件会进入 `session_events`，哪些事件会触发 projection checkpoint。

### 0.2 前端 Item Rendering 校准

检查点：

- `item_started` / `item_completed` 当前如何进入 `useSessionStream` / `useSessionFeed`。
- `ThreadItem::contextCompaction` 是否已有通用展示路径。
- ContextFrame `compaction_summary` 当前展示字段与扩展字段的差距。

产出：

- 明确 frontend MVP 是复用 generic item card，还是新增 compact item renderer。
- 明确 projection view 是本任务落 MVP skeleton，还是在后续 UI 子任务展开。

验收：

- compact started/completed 在 timeline 中有可见生命周期。
- ContextFrame 能展示 checkpoint/projection 关键 metadata。

### 0.3 Repository Transaction 边界校准

检查点：

- `SessionEventStore::append_event()` 当前自带事务，是否需要新增 application-level atomic commit primitive。
- Postgres / SQLite repository 是否可共享 projection store trait。
- migration 当前编号、测试数据库初始化路径和 generated SQL 风格。

产出：

- 决定 `append_compaction_commit()` 或等价 repository API 的事务边界。
- 明确 `session_compactions`、`session_projection_segments`、`session_projection_heads` 的 migration 文件位置和测试覆盖方式。

验收：

- completed lifecycle、checkpoint、segments、projection head 能作为一个原子提交单元设计。
- persist failure 的行为可以被单元测试稳定复现。

### 0.4 Branch / Lineage 命名校准

检查点：

- `04-08-session-tree-branching` 对 session tree、branch、lineage 的现有规划。
- 当前 session model 是否已有 workspace / branch 相关字段。
- checkpoint 需要先落 nullable `branch_id`，还是直接引入 lineage 表。

产出：

- 本任务 MVP 的 branch-aware 最小字段。
- 与后续 branch task 的命名对齐备注。

验收：

- checkpoint 与 projection head 至少能表达 `session_id + branch_id? + head_event_seq`。
- fork materialization 的数据坐标清晰，不依赖 parent 后续 compact 状态。

### 0.5 Token Estimator 校准

检查点：

- 现有 provider-visible token usage / estimator 是否可复用。
- `BeforeProviderRequestInput` 是否需要扩展 draft token estimate。
- `reserve_tokens` 如何进入 cut strategy。

产出：

- MVP token budget 策略。
- SummaryPrefixCompaction 的 cut input 契约。

验收：

- cut 不再以 `keep_last_n` 作为主预算模型。
- provider-visible pressure 覆盖 transform 后 draft request。

## Phase 1. Codex-aligned Compaction Lifecycle

目标：让所有 runtime compact 都进入统一 Backbone lifecycle，为后续 checkpoint store 提供控制面事实。

### 1.1 Protocol / Backbone

工作项：

- 校准 `BackboneEvent::ContextCompacted` 与 `ItemStarted` / `ItemCompleted` 的 compact 关系。
- 为 Pi/native compact 生成 `ThreadItem::contextCompaction` item id。
- 定义 compact failure diagnostic 的 payload 形态，优先使用现有 `BackboneEvent::Error` 或结构化 platform diagnostic。
- 重新生成 `packages/app-web/src/generated/backbone-protocol.ts`。

涉及文件：

- `crates/agentdash-agent-protocol/src/backbone/event.rs`
- `crates/agentdash-agent-protocol/src/compat/mod.rs`
- `packages/app-web/src/generated/backbone-protocol.ts`

验收：

- TS 生成后包含 compact lifecycle 所需类型。
- `contextCompaction` item lifecycle 能被 Backbone 序列化、持久化、前端消费。

### 1.2 Codex Bridge

工作项：

- 映射 `item/started` 中的 `contextCompaction`。
- 映射 `item/completed` 中的 `contextCompaction`。
- 接入当前真实 legacy completed marker method。
- 保留外部 Codex compact lifecycle 的 audit/feed 意义。

涉及文件：

- `crates/agentdash-executor/src/connectors/codex_bridge.rs`
- `crates/agentdash-executor/src/connectors/codex_bridge*tests*`

验收：

- Codex compact started/completed 能进入 `session_events`。
- Codex completed marker 不会被误用为 AgentDash-owned replacement projection。

### 1.3 Pi / Native Runtime

工作项：

- 将 `AgentEvent::ContextCompacted` 映射为 Codex-aligned compact item completed。
- 在 compaction 开始前发出 compact item started。
- platform `context_compacted` 的数据转为 ContextFrame/projected metadata 来源。

涉及文件：

- `crates/agentdash-agent/src/types.rs`
- `crates/agentdash-agent/src/agent_loop/streaming.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`

验收：

- Pi/native compact 在事件流中表现为 started/completed lifecycle。
- 现有 ContextFrame 展示能力可以继续从 compact metadata 派生。

## Phase 2. Durable Checkpoint / Projection Store

目标：引入一等 checkpoint / projection store，让 successful compact 具备 durable restore 事实。

### 2.1 数据模型与 Migration

工作项：

- 新增 `session_compactions`。
- 新增 `session_projection_segments`。
- 新增 `session_projection_heads`。
- 为 Postgres / SQLite 同步添加 migration。

建议字段以 `design.md` 为准，MVP 必须包含：

- `session_id`
- `branch_id?`
- `projection_kind`
- `projection_version`
- `status`
- `lifecycle_item_id`
- `start_event_seq`
- `completed_event_seq?`
- `source_start_event_seq`
- `source_end_event_seq`
- `first_kept_event_seq`
- `summary`
- `replacement_projection_json`
- `token_stats_json`
- `diagnostics_json`

涉及文件：

- `crates/agentdash-infrastructure/migrations/*`
- `crates/agentdash-infrastructure/src/persistence/postgres/*`
- `crates/agentdash-infrastructure/src/persistence/sqlite/*`
- `crates/agentdash-spi/src/session_persistence.rs`

验收：

- Postgres / SQLite schema 都能创建 projection store。
- repository tests 能插入、读取、更新 active projection head。

### 2.2 Store Trait

工作项：

- 定义 `SessionCompactionStore`。
- 定义 `SessionProjectionStore`。
- 组合进 application session persistence 装配层。
- 提供 atomic commit API：

```text
commit_compaction_projection {
  completed_event
  compaction_record
  projection_segments
  projection_head_update
}
```

验收：

- 成功 commit 后 event、checkpoint、segments、head 一致可读。
- 任一写入失败时 active projection head 保持原值。

### 2.3 ContextFrame Derivation

工作项：

- `ContextFrame(kind="compaction_summary")` 从 compaction checkpoint / projection metadata 派生。
- section 增加 checkpoint id、projection version、strategy、trigger、phase、source range、first kept、tokens before/after。

涉及文件：

- `crates/agentdash-application/src/session/compaction_context_frame.rs`
- `packages/app-web/src/features/session/model/contextFrame.ts`
- `packages/app-web/src/features/session/ui/contextFrame/*`

验收：

- compact summary frame 展示结构化 metadata。
- frame 不再承担恢复事实源职责。

## Phase 3. ContextProjector / AgentContextEnvelope

目标：把 continuation 中的临时 projection 升级为正式模型上下文投影层。

### 3.1 Projection DTO

工作项：

- 扩展 `ProjectionKind`：`ModelContext`、`Timeline`、`Audit`、`Handoff`。
- 扩展 `ProjectedEntry` provenance：origin、synthetic、source range、segment id。
- 引入 `AgentContextEnvelope` / `AgentInputMessage`。

涉及文件：

- `crates/agentdash-agent-types/src/model/projection.rs`
- `crates/agentdash-agent-types/src/lib.rs`

验收：

- projection entry 可以区分真实事件和派生 segment。
- `.into_messages()` 只作为 materialization 目标之一，而不是 projection 的唯一形态。

### 3.2 ContextProjector

工作项：

- 将 `build_projected_transcript_from_events` 提升为正式 `ContextProjector`。
- Resume 时读取 active projection head。
- 加载有效 compaction + segments。
- replay checkpoint 后 suffix events。
- 输出 `AgentContextEnvelope`。

涉及文件：

- `crates/agentdash-application/src/session/continuation.rs`
- `crates/agentdash-application/src/session/eventing.rs`
- 新增 `crates/agentdash-application/src/session/context_projector.rs`

验收：

- continuation 不再从 platform `context_compacted` payload 反推 checkpoint。
- `[summary_chunk] + suffix` 可以从 checkpoint store 重建。
- 无 checkpoint 时仍可从 events 构建完整 projection。

### 3.3 ContextMaterializer

工作项：

- 定义 materializer 输入输出契约。
- 为 Pi/native bridge 输出 `Vec<AgentMessage>`。
- 为 provider-visible pressure 提供 draft materialized request。

验收：

- agent loop 不直接裁剪 UI timeline。
- provider request 来自 materialized projection。

## Phase 4. Agent Loop Compaction 重构

目标：让 compact 在正确的运行时边界发生，并保证失败不污染 active projection。

### 4.1 Pressure Evaluation Relocation

工作项：

- 将最终 pressure evaluation 移到 `transform_context` / draft materialization 之后。
- `system_prompt`、messages、tools、hook steering、ContextFrame 注入都进入估算。
- `BeforeProviderRequestInput` 可携带 token estimate / projection id。

涉及文件：

- `crates/agentdash-agent/src/agent_loop/streaming.rs`
- `crates/agentdash-agent-types/src/runtime/decisions.rs`
- `crates/agentdash-application/src/session/hook_delegate.rs`

验收：

- 压缩触发基于 provider-visible payload。
- 现有 transform_context 注入不被压缩前置逻辑遗漏。

### 4.2 SummaryPrefixCompaction

工作项：

- 替换 `keep_last_n` 主导 cut 为 token-budget cut。
- 使用 event/message ref 记录 source range 和 first kept pointer。
- 保证 tool call / tool result 因果边界。
- 空摘要返回 `summary_empty` failure。

涉及文件：

- `crates/agentdash-agent/src/compaction/mod.rs`
- `crates/agentdash-agent-types/src/runtime/decisions.rs`

验收：

- 空摘要不会产生成功 `CompactionSummary`。
- cut 结果可追溯到 event_seq / MessageRef。
- `reserve_tokens` 实际影响 retained tail。

### 4.3 CompactionOrchestrator

工作项：

- 在 application 层实现 compaction orchestration。
- 统一 lifecycle emit、strategy run、checkpoint commit、ContextFrame derive。
- 失败写 diagnostic，并保持 active projection head。

涉及文件：

- `crates/agentdash-application/src/session/*`
- `crates/agentdash-executor/src/connectors/pi_agent/*`

验收：

- successful compact 后 runtime 安装新的 projection。
- persist failure / summary failure 后 runtime 继续使用原 projection。

## Phase 5. Frontend Surface

目标：前端能区分真实 timeline 与模型可见 projection。

### 5.1 Timeline Compact Lifecycle

工作项：

- 渲染 `contextCompaction` started/completed item。
- compact marker 显示 trigger、phase、strategy、status。
- `context_compacted` neutral event 语义收敛到 lifecycle / ContextFrame。

涉及文件：

- `packages/app-web/src/features/session/model/useSessionStream.ts`
- `packages/app-web/src/features/session/model/useSessionFeed.ts`
- `packages/app-web/src/features/session/ui/SessionEntry*`

验收：

- 用户能看到 compact 正在发生与已完成。
- timeline 仍保留真实历史。

### 5.2 Projection View MVP

工作项：

- 新增 projection view DTO / API 或复用 session detail endpoint 扩展。
- 展示 model context projection segments。
- 标记 summary、pruned、original、artifact reference。

验收：

- 用户能看到“模型当前看到什么”。
- projection segment 可跳转到 source range 或相关 artifact。

## Phase 6. Branch-aware Projection

目标：为 session tree / branch / rollback 提供模型上下文基线。

工作项：

- checkpoint / projection head 绑定 branch 坐标。
- fork 时 materialize child initial projection。
- rollback 更新 active projection head。
- agent turn 记录 projection version / snapshot id。

涉及文件：

- `crates/agentdash-application/src/session/*`
- `crates/agentdash-spi/src/session_persistence.rs`
- `crates/agentdash-infrastructure/src/persistence/*`
- 后续对齐 `.trellis/tasks/04-08-session-tree-branching/`

验收：

- branch compact 互不污染 active projection。
- rollback 后 resume 使用新的 projection head。
- audit 能回答某 turn 使用的 projection。

## Phase 7. Strategy Expansion

目标：在基础设施稳定后接入更高阶压缩策略。

### 7.1 ToolResultPruning

- 大型 tool output 入 artifact store。
- projection 中保留 digest、artifact reference、关键 metadata。
- timeline 仍能查看完整工具结果。

### 7.2 ReactiveEmergencyCompact

- provider overflow 后记录 diagnostic。
- 对失败 draft request 建立 emergency projection。
- 成功后重试原 turn。

### 7.3 BranchHandoffSummary

- 区分 model context summary、branch summary、handoff summary。
- 三者共享 source events，写入不同 projection kind。

### 7.4 ProviderNativeCompaction

- provider compact 输出归一化为 projection segments。
- 外部 runtime 没有 replacement provenance 时只作为 lifecycle/audit event。

## Validation Commands

实施阶段每轮按影响面选择执行：

```powershell
cargo test -p agentdash-agent
cargo test -p agentdash-agent-types
cargo test -p agentdash-agent-protocol
cargo test -p agentdash-application
cargo test -p agentdash-infrastructure
pnpm test
pnpm typecheck
```

协议类型变更后执行：

```powershell
cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts
```

需要端到端验证时使用：

```powershell
pnpm dev
```

## High-risk Files

- `crates/agentdash-agent/src/agent_loop/streaming.rs`
- `crates/agentdash-agent/src/compaction/mod.rs`
- `crates/agentdash-agent-types/src/model/projection.rs`
- `crates/agentdash-agent-types/src/runtime/decisions.rs`
- `crates/agentdash-agent-protocol/src/backbone/event.rs`
- `crates/agentdash-executor/src/connectors/codex_bridge.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`
- `crates/agentdash-application/src/session/continuation.rs`
- `crates/agentdash-application/src/session/eventing.rs`
- `crates/agentdash-application/src/session/hook_delegate.rs`
- `crates/agentdash-spi/src/session_persistence.rs`
- `crates/agentdash-infrastructure/migrations/*`
- `packages/app-web/src/generated/backbone-protocol.ts`
- `packages/app-web/src/features/session/model/*`
- `packages/app-web/src/features/session/ui/*`

## Review Gates

### Gate A. Planning Ready

- `prd.md`、`design.md`、`implement.md` 三者一致。
- Phase 0 待校准点都有明确 owner 和验收方式。
- 用户确认可以进入实现阶段。

### Gate B. Protocol Ready

- Codex Bridge 和 Pi/native compact lifecycle 对齐 Backbone。
- 前端能消费 compact item lifecycle。
- generated TS 与 Rust types 同步。

### Gate C. Store Ready

- projection store migrations 和 repository tests 通过。
- compaction commit 事务语义被测试覆盖。

### Gate D. Projector Ready

- resume 使用 checkpoint + suffix。
- ContextFrame 从 checkpoint metadata 派生。
- agent input 与 UI timeline 分离。

### Gate E. Runtime Ready

- pressure evaluation 覆盖 provider-visible draft request。
- failure 不安装 active projection。
- successful compact 可恢复、可审计、可前端解释。

## First Implementation Slice

推荐第一段实际开发只覆盖 MVP 基座：

1. Phase 0 校准 compact method、frontend item renderer、transaction boundary。
2. Phase 1 完成 compact lifecycle 对齐。
3. Phase 2 落 `session_compactions` / `session_projection_segments` / `session_projection_heads`。
4. Phase 3 让 continuation resume 从 checkpoint store 恢复。
5. Phase 4 修正空摘要成功和 provider-visible pressure 的最小闭环。

完成这段后，AgentDash 就具备真正可恢复的 summary compaction 基础。ToolResultPruning、branch handoff、provider-native compact 再作为后续 slice 接入同一套 projection infrastructure。
