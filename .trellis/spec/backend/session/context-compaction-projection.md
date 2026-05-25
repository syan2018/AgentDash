# Context Compaction Projection

## Scope

上下文压缩投影契约适用于平台自有 Pi/native runtime 的结构性 compact、session resume、模型上下文查询和后续 branch / rollback 基线。外部 runtime 只在能提供 replacement provenance 时导入 AgentDash-owned projection；否则只作为 Backbone lifecycle / audit fact 保存。

## Durable Shape

`session_events` 保存真实发生过的事实：用户输入、assistant 输出、工具生命周期、compact lifecycle、failure diagnostic 和 ContextFrame。compact 不改写历史事件，只提交新的模型上下文 projection。

成功结构性 compact 使用三类持久化对象表达可恢复状态：

| Store | 职责 |
| --- | --- |
| `session_compactions` | checkpoint-oriented record，记录 lifecycle item、status、strategy、trigger、phase、source range、first kept pointer、token stats、summary 与 replacement projection metadata |
| `session_projection_segments` | 可恢复投影片段，当前 MVP 至少写入 `summary_chunk`，后续策略可扩展为 `pruned_message`、`tool_result_digest`、`artifact_reference` |
| `session_projection_heads` | 当前 projection kind 的 active cursor，记录 `projection_version`、`head_event_seq` 与 `active_compaction_id` |

PostgreSQL 与 SQLite repository 必须把 compact completed event、compaction record、projection segments 和 projection head 放在同一提交单元中。提交失败时 active projection head 保持原值。

## Runtime Contract

Pi/native compact 进入 Codex-aligned item lifecycle：

```text
ContextCompactionStarted
  -> BackboneEvent::ItemStarted(ThreadItem::contextCompaction)

ContextCompacted
  -> PlatformEvent::SessionMetaUpdate(key = "context_compacted")
  -> BackboneEvent::ItemCompleted(ThreadItem::contextCompaction)

ContextCompactionFailed
  -> PlatformEvent::SessionMetaUpdate(key = "context_compaction_failed")
  -> BackboneEvent::Error
```

应用层在持久化 `context_compacted` 时提交 checkpoint / segments / head，并由该 metadata 派生 `ContextFrame(kind="compaction_summary")`。checkpoint 提交完成后再让 item completed 进入普通事件流，这样 resume 不会看到只有 completed marker、没有恢复事实的状态。

`context_compaction_failed` 是结构化 diagnostic。失败不会生成 `session_compactions(status = projection_committed)`，也不会替换 active projection head。Hook runtime 会记录连续失败次数，达到阈值后暂停后续自动压缩尝试；成功 compact 会复位该计数。

## ContextProjector

模型输入由 `ContextProjector` 从 durable facts 构建，而不是从 UI timeline message array 裁剪。读取顺序：

```text
session_projection_heads(model_context)
  -> active session_compactions
  -> session_projection_segments
  -> suffix session_events
  -> AgentContextEnvelope
```

没有 active projection head 时，ContextProjector 从 `session_events` 构建完整 transcript projection。

`AgentContextEnvelope` 内的每条 `AgentInputMessage` 必须携带 provenance：

| Field | 含义 |
| --- | --- |
| `origin` | `event` 表示真实事件投影；`projection` 表示摘要、裁剪或 digest 等派生片段 |
| `synthetic` | 派生模型输入为 `true` |
| `source_event_seq` / `source_range` | 该条模型输入来源的事实事件坐标 |
| `projection_segment_id` | 派生内容对应的 projection segment |
| `active_compaction_id` | envelope 当前使用的 active checkpoint |

API `GET /sessions/{id}/context/projection` 返回当前 `model_context` projection view，前端 Context panel 用它展示模型当前可见 segments。Timeline 继续消费真实事件流，两者不互相替代。

## Branch Baseline

本任务的 projection store 已为 branch-aware restore 保留 `branch_id`、`head_event_seq` 与 active compaction cursor。完整 fork / rollback / lineage API 由 session tree branching 任务承接；该任务应消费 `session_compactions` 的 checkpoint surface 与 `session_projection_heads`，而不是重新从 timeline 推导模型可见状态。
