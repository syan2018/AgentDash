# 上下文压缩 checkpoint 解析层收敛

## Goal

收敛上下文压缩 checkpoint 的解析与边界语义，让 durable projection、continuation restore、后续 fork / rollback 能复用同一套业务解释，避免 `messages_compacted`、`compacted_until_ref`、`source_range` 在不同模块中各自推导并产生分叉。

## Requirements

- 在 `agentdash-application` 内新增内部 `compaction_checkpoint` 模块，不修改数据库 schema、HTTP API、前端 contract 或公开 SPI 类型。
- 统一解析 projection segment、compaction record、`context_compacted` event payload 中的 checkpoint metadata。
- 固化字段优先级：
  - `messages_compacted`: segment `source_refs_json.messages_compacted` > compaction `token_stats_json.messages_compacted` > event payload `messages_compacted` > `0`。
  - `compacted_until_ref`: segment `source_refs_json.compacted_until_ref` > compaction `replacement_projection_json.compacted_until_ref` > event payload `compacted_until_ref` > `None`。
  - `source_range`: segment source range > compaction source range > `None`。
  - `summary`: segment `content_json.content/summary` > compaction `summary` > event payload `summary`。
- `ContextProjector` 使用 checkpoint parser 构造 `AgentMessage::CompactionSummary` 的 `ProjectedEntry`，不再直接解析 checkpoint JSON。
- `continuation.rs` 使用 checkpoint parser 发现最新 `context_compacted` checkpoint，并复用统一裁剪逻辑。
- 保持现有行为：durable projection 仍恢复为 summary segment + suffix transcript；event continuation 仍用最新 checkpoint 裁剪旧消息；缺失 summary 或 `messages_compacted == 0` 时不应用 checkpoint。
- 当前项目未上线，不新增历史兼容分支；已有 draft 数据按最新正确语义重建。

## Acceptance Criteria

- [ ] `ContextProjector` 与 `continuation.rs` 不再各自维护 checkpoint JSON 字段优先级。
- [ ] Projection segment metadata 优先于 compaction metadata，并由单元测试覆盖。
- [ ] Compaction metadata 可作为 segment 缺失时的 fallback，并由单元测试覆盖。
- [ ] `context_compacted` event payload 可解析为 checkpoint，并由 continuation 侧测试覆盖。
- [ ] 非法 `compacted_until_ref` 和非法 source range 有明确失败路径，且不会静默生成错误边界。
- [ ] 现有 compaction projection、projection view、SessionChatView 刷新测试继续通过。

## Supplemental Material

- `references/arch-review.md`：当前上下文压缩和持久化架构图，以及 checkpoint 解析层的动机说明。

## Out of Scope

- 不实现 #33 的 session fork / rollback。
- 不修改 migration、数据库字段或 API response wire shape。
- 不重写 token 估算职责；`ContextProjector` 仍负责当前模型可见上下文 token estimate。
