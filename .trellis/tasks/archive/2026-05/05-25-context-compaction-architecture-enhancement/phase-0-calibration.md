# Phase 0 决策锚点校准

## Purpose

本文把 `prd.md`、`design.md`、`implement.md` 中的架构意图落成可施工的最小契约。Phase 0 的职责不是完成实现，而是确认哪些事实已经足够支撑 Phase 1-4 的重构顺序。

本轮校准后的推进判断：

- Phase 1 先收敛 compact lifecycle：所有 runtime 都进入 `item/started` / `item/completed` 的 `contextCompaction` item 生命周期。
- Phase 2 再落 checkpoint / projection store：成功 compact 的恢复事实由仓储一次性提交。
- Phase 3 把 continuation projection 提升为正式 ContextProjector。
- Phase 4 修正 provider-visible pressure 与 summary failure 语义。

## 1. Codex Protocol Method

已确认的参考事实：

- Codex app-server 的 compact 主路径是 `item/started` 与 `item/completed`，payload item 为 `{ type: "contextCompaction", id }`。
- Codex protocol 中 deprecated completed marker 的 method 是 `thread/compacted`。
- `ContextCompactedNotification` 只有 `{ threadId, turnId }`，不足以表达 replacement projection。

当前项目事实：

- `BackboneEvent` 已经有 `ItemStarted`、`ItemCompleted`、`ContextCompacted`。
- 生成的 `packages/app-web/src/generated/backbone-protocol.ts` 已包含 `ThreadItem` 的 `contextCompaction` union。
- `CodexBridgeConnector` 已经泛化映射 `item/started` 和 `item/completed`。
- 当前 `CodexBridgeConnector` 的 legacy compact marker 分支监听的是 `context/compacted`，与参考协议中的 `thread/compacted` 不一致。

施工契约：

- Phase 1 以 `item/* contextCompaction` 作为 compact lifecycle 的主信号。
- Codex Bridge 的 legacy completed marker 收敛到 `thread/compacted`。
- Codex Bridge 只把 legacy marker 作为外部 runtime lifecycle / audit 事实持久化；在没有 replacement history、source range、first kept pointer 时，不创建 AgentDash-owned checkpoint。

## 2. Frontend Item Rendering

当前项目事实：

- `useSessionStream` 会按 `ThreadItem.id` 合并 `item_started` / `item_completed`。
- `SessionEntry` 会把非 commandExecution 的 `ThreadItem` 交给 `SessionToolCallCard`。
- `contextCompaction` 目前没有专门的 title / status / kind 映射，通用路径会显示为 `未知` / `completed` / `other`。
- `useSessionFeed` 的 tool-like 聚合只覆盖 command、file、mcp、dynamic、webSearch、image 等 item；`contextCompaction` 不会被错误归入工具聚合。

施工契约：

- Phase 1 只需要补齐 `contextCompaction` 的显示语义，即可让 timeline 看见 compact lifecycle。
- `contextCompaction` 在 timeline 中作为可见 lifecycle marker，而不是 tool burst 的一部分。
- Projection View 不并入 Phase 1；它依赖 Phase 2 / Phase 3 的 projection store 与 ContextProjector。

## 3. Native Runtime Lifecycle

当前项目事实：

- native agent loop 在 `transform_context` 前调用 `evaluate_compaction -> execute_compaction`。
- 成功 compact 后直接执行 `context.messages = result.messages.clone()`。
- `AgentEvent::ContextCompacted` 是成功后的单个事件，没有 started lifecycle。
- Pi stream mapper 当前把 `AgentEvent::ContextCompacted` 映射为 `PlatformEvent::SessionMetaUpdate { key: "context_compacted" }`。
- application 层依赖 `context_compacted` platform payload 派生 `ContextFrame(kind="compaction_summary")`，continuation 也从该 payload 反推出临时 checkpoint。

施工契约：

- Phase 1 为 native compact 增加 started / completed item lifecycle，并保证同一次 compact 使用同一个 item id。
- Phase 1 继续保留结构化 compact metadata 作为 ContextFrame 的当前数据源；它是 Phase 2 前的解释层输入，不是最终恢复事实源。
- Phase 2 完成后，ContextFrame 改为从 `session_compactions` / `session_projection_segments` 派生。

## 4. Compaction Strategy Baseline

当前项目事实：

- `execute_compaction()` 使用 `keep_last_n` 决定 cut point。
- `reserve_tokens` 已进入 `CompactionParams`，但当前 cut strategy 没有实际使用它。
- cut point 已有基础 tool call / tool result 因果保护。
- summary 生成为空时会写入占位成功摘要。

施工契约：

- Phase 4 将最终 pressure evaluation 移到 `transform_context` 与 draft materialization 之后。
- Phase 4 让 `reserve_tokens` 进入 retained tail / cut 决策。
- summary 为空应成为结构化 failure diagnostic，不能生成成功 checkpoint 或成功 ContextFrame。

## 5. Repository Transaction Boundary

当前项目事实：

- `SessionEventStore::append_event()` 是事实事件写入入口。
- PostgreSQL 与 SQLite 的 `append_event()` 都各自开启事务，递增 `sessions.last_event_seq`，写入 `session_events`，再更新 `sessions` projection fields。
- in-memory persistence 也把 `append_event()` 当作单个原子更新。
- 现有 trait 没有 event append + compaction checkpoint + projection segments + projection head 的统一提交原语。

施工契约：

- Phase 2 新增 session compaction / projection store trait，并提供应用需要的原子提交 API。
- 成功 compact 的 completed lifecycle event、compaction record、projection segments、projection head update 必须在同一提交单元中完成。
- persist failure 时 active projection head 保持原值，失败事实通过 diagnostic event 记录。

## 6. Storage Naming

设计收口：

- 本任务使用 `session_compactions` 承担 checkpoint-oriented record 职责。
- `session_projection_segments` 保存 summary chunk、kept tail、pruned message、artifact reference 等派生片段。
- `session_projection_heads` 保存 active model-visible cursor。

与后续 branch 任务的对齐：

- `.trellis/tasks/04-08-session-tree-branching` 中提到的 `session_checkpoints` 语义，在本任务中由 `session_compactions` 提供。
- branch 后续实现应消费 checkpoint surface，而不是依赖表名必须叫 `session_checkpoints`。
- fork point 字段优先表达为 `fork_point_compaction_id` / `active_compaction_id` 这类 checkpoint 语义坐标。

## 7. Continuation / ContextProjector Baseline

当前项目事实：

- `build_projected_transcript_from_events()` 已能从 `session_events` 重建 transcript。
- `ProjectedEntry` 已包含 `message_ref` 与 `projection_kind`。
- `ProjectionKind` 目前只有 `Transcript` 与 `CompactionSummary`。
- continuation 当前通过 `PlatformEvent::SessionMetaUpdate(key="context_compacted")` 提取 summary、tokens、messages_compacted、compacted_until_ref，再拼出 `[summary] + suffix`。

施工契约：

- Phase 3 把 `build_projected_transcript_from_events()` 提升为正式 ContextProjector 的核心输入之一。
- ContextProjector 的 checkpoint 输入来自 projection store，而不是 platform payload。
- projection DTO 扩展 origin、synthetic、source range、segment id 等 provenance。

## 8. Token / Pressure Baseline

当前项目事实：

- runtime delegate 通过 hook session token stats 提供 `last_input_tokens` 与 `context_window`。
- `BeforeProviderRequestInput` 目前只有 system prompt 长度、message count、tool count。
- provider bridge 能在调用后产出 usage，但当前缺少对 draft provider request 的 provider-visible token estimate。

施工契约：

- Phase 4 引入 draft request 级 pressure planning。
- MVP 可以先使用保守 estimate，但触发点必须位于 transform / materialize 之后。
- 后续 provider-specific tokenizer 可以替换 estimate 实现，不改变 ContextProjector / checkpoint store 契约。

## 9. Phase Commit Order

阶段提交顺序：

1. `docs(compaction): 完成重构目标校准`
   - 提交本文与任务状态。
2. `feat(compaction): 对齐上下文压缩生命周期`
   - Codex Bridge legacy method、native started/completed lifecycle、frontend compact item display。
3. `feat(compaction): 新增压缩投影仓储基座`
   - migrations、SPI traits、repository tests。
4. `feat(compaction): 引入上下文投影恢复路径`
   - ContextProjector / checkpoint + suffix resume。
5. `fix(compaction): 按 provider 可见上下文触发压缩`
   - pressure relocation、empty summary failure、token-budget cut。

每个阶段完成后重新阅读任务文档，再继续下一阶段。
