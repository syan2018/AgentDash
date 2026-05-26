# 上下文索引

## 目标

评估当前项目 compaction、session branching / lineage 模块是否足够 solid、是否易扩展，并与 `references/codex`、`references/claude-code`、`references/pi-mono` 的相关实现做结构对照。

## 阶段记录

| 阶段 | 状态 | 产物 |
| --- | --- | --- |
| 1. 代码定位 | 已完成 | 当前项目与 reference 关键文件索引 |
| 2. subagent 调研 | 已完成 | 四个只读 explorer 分别覆盖当前 compaction、当前 lineage、Codex reference、Claude/pi-mono reference |
| 3. 主线交叉阅读 | 已完成 | 数据流、边界、风险核验 |
| 4. 汇总判断 | 已完成 | findings 与扩展性建议 |

## Subagent 分工

| 代号 | 身份 | 范围 |
| --- | --- | --- |
| Anscombe | AgentDashboard compaction 调研员 | 当前项目 compaction / projection / context frame |
| Schrodinger | AgentDashboard session branching 调研员 | 当前项目 lineage / fork / parent-child session |
| Euler | Codex reference 对照调研员 | `references/codex` |
| Plato | Claude/pi-mono reference 对照调研员 | `references/claude-code`、`references/pi-mono` |

## 继续 Review 的最短路径

如果上下文被压缩或后续会话接手，按这个顺序恢复状态：

1. 读 `03-findings.md`，先获得风险排序和推荐路线。
2. 读 `01-current-implementation.md` 的 “Compaction 数据流” 与 “Session Branching / Lineage 数据流”，确认当前实现事实。
3. 读本文件的 “当前项目关键索引”，按 finding 需要跳转到具体代码。
4. 需要做设计取舍时再读 `02-reference-comparison.md`，不要直接照搬 reference 的兼容逻辑。

## 当前项目关键索引

### Compaction

- `crates/agentdash-agent/src/compaction/mod.rs`
- `crates/agentdash-agent/src/agent.rs`
- `crates/agentdash-agent/src/types.rs`
- `crates/agentdash-agent/src/agent_loop/streaming.rs`
- `crates/agentdash-agent-types/src/model/message.rs`
- `crates/agentdash-agent-types/src/model/projection.rs`
- `crates/agentdash-spi/src/session_persistence.rs`
- `crates/agentdash-application/src/session/compaction_checkpoint.rs`
- `crates/agentdash-application/src/session/compaction_context_frame.rs`
- `crates/agentdash-application/src/session/context_projector.rs`
- `crates/agentdash-application/src/session/eventing.rs`
- `crates/agentdash-application/src/session/hook_delegate.rs`
- `crates/agentdash-application/scripts/hook-presets/context_compaction_trigger.rhai`
- `crates/agentdash-application/src/session/memory_persistence.rs`
- `crates/agentdash-application/src/session/persistence.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`
- `crates/agentdash-executor/src/connectors/codex_bridge.rs`
- `crates/agentdash-infrastructure/migrations/0059_session_compaction_projection_store.sql`
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`
- `crates/agentdash-infrastructure/src/persistence/sqlite/session_repository.rs`
- `packages/app-web/src/generated/backbone-protocol.ts`
- `packages/app-web/src/features/session/ui/SessionProjectionView.tsx`
- `packages/app-web/src/features/session/ui/ContextFrameCard.tsx`
- `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx`
- `packages/app-web/src/features/session/ui/SessionChatView.tsx`
- `packages/app-web/src/features/session/model/contextFrame.ts`
- `packages/app-web/src/services/session.ts`
- `packages/app-web/src/generated/session-contracts.ts`

### Session Branching / Lineage

- `crates/agentdash-spi/src/session_persistence.rs`
- `crates/agentdash-contracts/src/session.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`
- `crates/agentdash-api/src/routes/project_sessions.rs`
- `crates/agentdash-api/src/session_use_cases/construction.rs`
- `crates/agentdash-application/src/session/branching.rs`
- `crates/agentdash-application/src/session/context_projector.rs`
- `crates/agentdash-application/src/session/memory_persistence.rs`
- `crates/agentdash-application/src/session/persistence.rs`
- `crates/agentdash-infrastructure/migrations/0060_session_lineage.sql`
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`
- `crates/agentdash-infrastructure/src/persistence/sqlite/session_repository.rs`
- `packages/app-web/src/features/session/ui/SessionLineageView.tsx`
- `packages/app-web/src/features/session/ui/SessionChatView.tsx`
- `packages/app-web/src/features/agent/active-session-list.tsx`
- `packages/app-web/src/features/agent/session-relations.ts`
- `packages/app-web/src/features/agent/session-grouping.ts`
- `packages/app-web/src/services/session.ts`
- `packages/app-web/src/generated/session-contracts.ts`

## Reference 关键索引

### Codex

- `references/codex/codex-rs/core/src/session/turn.rs` - auto compact token status、pre-sampling compact、mid-turn compact。
- `references/codex/codex-rs/core/src/compact.rs` - compact task、summary 生成、replacement history、initial context injection。
- `references/codex/codex-rs/core/src/session/mod.rs` - replace compacted history、initial history、resume/fork reconstruction 入口。
- `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs` - 从 rollout checkpoint / rollback marker 重建 history。
- `references/codex/codex-rs/protocol/src/protocol.rs` - `CompactedItem`、`InitialHistory`、`ResumedHistory`、`TokenUsageInfo`。
- `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs` - `thread/compact/start`、resume/fork response 与 usage replay。
- `references/codex/codex-rs/thread-store/src/types.rs` - thread store fork/resume 参数。
- `references/codex/codex-rs/agent-graph-store/src/store.rs` - subagent thread spawn edge store。
- `references/codex/codex-rs/agent-graph-store/src/types.rs` - thread spawn edge status。

### Claude Code

- `references/claude-code/src/services/compact/autoCompact.ts` - 自动 compaction 阈值、输出预留、递归保护、失败熔断。
- `references/claude-code/src/services/compact/compact.ts` - full/partial compaction、post-compact 附件恢复、forked-agent cache sharing。
- `references/claude-code/src/services/compact/prompt.ts` - compact prompt 结构、`<analysis>` 剥离、summary message 格式。
- `references/claude-code/src/utils/sessionStorage.ts` - JSONL transcript、`parentUuid` 链、leaf resume、compact boundary。
- `references/claude-code/src/commands/branch/branch.ts` - `/branch` 新 session fork、`forkedFrom` 溯源、parentUuid 重写。
- `references/claude-code/src/utils/forkedAgent.ts` - subagent context clone、cache-safe 参数。
- `references/claude-code/src/tools/AgentTool/AgentTool.tsx` - fork subagent、agentId、异步/同步 agent 输出。

### pi-mono

- `references/pi-mono/packages/coding-agent/src/core/session-manager.ts` - append-only entry tree、compaction entry、branch summary、context rebuild。
- `references/pi-mono/packages/coding-agent/src/core/agent-session.ts` - manual/auto/extension compaction、tree navigation、branch summary。
- `references/pi-mono/packages/coding-agent/src/core/compaction/compaction.ts` - iterative summary、cut point、file ops detail、turn-prefix summary。
- `references/pi-mono/packages/coding-agent/src/core/compaction/branch-summarization.ts` - abandoned branch summary。
- `references/pi-mono/packages/coding-agent/src/core/agent-session-runtime.ts` - resume/fork runtime replacement lifecycle。
- `references/pi-mono/packages/coding-agent/src/modes/interactive/components/tree-selector.ts` - explicit session tree UI。
- `references/pi-mono/packages/coding-agent/examples/extensions/subagent/` - isolated subagent subprocess example。

## 已确认问题入口

- 当前项目的 `pi_agent` compaction 链路完整；Codex 原生 `thread/compacted` 链路只映射为 `BackboneEvent::ContextCompacted`，缺少 summary/boundary，不会进入 projection commit。
- `branch_id` 与 `session_lineage` 是两套层级：前者是 projection store 维度，当前 live/fork/rollback 路径都传 `None`；后者是跨 session parent-child edge。
- fork 出来的 child session 通过 `fork_initial_projection` 的 `context_envelope` segment 获得父会话当时的 model context，不复制父会话原始事件流。
- reference 的共同启发是：compaction boundary/checkpoint、fork/replay reducer、usage snapshot、lineage graph 要做成一等语义，而不是靠消息文本隐式推断。

## 已校准的非问题

- `fork_initial_projection` 构造时 `completed_at_ms: None`、head `updated_at_ms: 0` 是提交前占位；SQLite/Postgres `commit_compaction_projection` 会填入真实 `committed_at_ms`。
- `branch_id: None` 在当前实现里不是 bug；它表示当前没有启用同 session branch namespace。真正需要处理的是长期命名和语义边界。
