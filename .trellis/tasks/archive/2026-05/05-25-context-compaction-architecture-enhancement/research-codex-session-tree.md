# Codex Session Tree 与 Rollout 状态调研

## 参考文件

- `references/codex/codex-rs/agent-graph-store/src/store.rs`
- `references/codex/codex-rs/agent-graph-store/src/local.rs`
- `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs`
- `references/codex/codex-rs/core/src/session/handlers.rs`
- `references/codex/codex-rs/core/src/session/mod.rs`
- `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs`
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs`

## Codex 的会话状态模型

Codex 的主事实源是 rollout JSONL。thread/fork、thread/resume、thread/rollback 都围绕“读取 rollout、重建历史、继续写入 rollout”展开。

- `thread/resume` 从 thread id 或 rollout path 读取完整 persisted history，包装成 `InitialHistory::Resumed`，再由 core session 执行 rollout reconstruction。
- `thread/fork` 读取源 thread 的 persisted history，新建 thread，并把源历史作为 forked thread 的初始历史。持久化 fork 会立即 materialize 自己的 rollout；ephemeral fork 则 pathless，用复制来的 source history 构建可见 turns。
- `thread/rollback` 不直接删除 rollout 历史，而是追加 `ThreadRolledBack` event，然后通过 replay 把 runtime history 重建为“逻辑上已经丢掉最后 N 个 user turns”的状态。
- `rollout_reconstruction` 会从新到旧扫描 rollout，找到最新 surviving `replacement_history` compaction checkpoint，再只向前 replay checkpoint 之后的 suffix；rollback marker 会在反向扫描中变成“跳过接下来 N 个 user turn segment”。

Codex 的 parent/child topology 独立于 rollout 存在于 `AgentGraphStore`：

- 每个 child thread 最多一个 persisted parent。
- edge 有 `open` / `closed` status。
- list children 和 descendants 要保证稳定顺序，descendants 按 breadth-first depth 再 thread id 排序。
- 这个 graph 更像 spawned agent topology 索引，不承载完整 transcript 状态。

## 对 AgentDash 的启发

AgentDash 不应该照搬“文本 rollout 即事实源”的做法。我们已经有 `sessions`、`session_events`、`SessionMeta.last_event_seq` 和 repository abstraction，更适合采用“不可变事件日志 + 结构化状态索引 / checkpoint 表”的模型。

建议把三类东西分清：

- `session_events`：不可变审计日志，记录 Backbone/Platform 事件，负责 feed、ContextFrame、回放审计。
- `session_checkpoints`：模型可恢复状态快照，负责 restore / continuation / fork base，不从 UI 文本里反推。
- `session_lineage`：会话分支拓扑，负责 parent/child、fork point、branch status、spawn/fork/companion 关系。

这个模型和 Codex 的对应关系是：

- Codex rollout JSONL -> AgentDash `session_events`。
- Codex replacement_history compaction item -> AgentDash `session_checkpoints`。
- Codex AgentGraphStore edge -> AgentDash `session_lineage`。
- Codex ThreadRolledBack event -> AgentDash `session_state_transition` 或 `session_events` 中的 rollback platform event + active projection cursor。

## 数据库仓储建议

### session_checkpoints

用于恢复和压缩，不是纯 UI 事件。

概念字段：

- `checkpoint_id`
- `session_id`
- `created_event_seq`
- `covered_until_event_seq`
- `covered_until_ref`
- `base_checkpoint_id`
- `branch_root_session_id`
- `lineage_node_id`
- `status`: `active | superseded | rolled_back | failed`
- `replacement_projection_json`
- `summary`
- `token_stats_json`
- `strategy`
- `phase`
- `created_at_ms`

恢复时按当前 active projection cursor 查找最新有效 checkpoint，再 replay checkpoint 之后且没有被 rollback 排除的 session events。

### session_lineage

用于表达 session tree，不替代 owner binding / companion context。

概念字段：

- `child_session_id`
- `parent_session_id`
- `relation_kind`: `fork | companion | spawned_agent | rollback_branch`
- `fork_point_event_seq`
- `fork_point_ref`
- `fork_point_checkpoint_id`
- `status`: `open | closed | archived`
- `created_at_ms`
- `metadata_json`

child session 最多一个 primary parent，便于树形 UI 和恢复策略；同一个 session 仍可通过 `session_bindings` 绑定到 project/story/task owner。

### active projection cursor

Rollback 不应删除历史事件。更好的做法是写入 rollback transition，并更新一个“当前模型可见状态”的 cursor。

概念字段可以放在 `session_projection_heads`，也可以收进 checkpoint 表或 session meta：

- `session_id`
- `projection_kind`: `model_visible | ui_visible`
- `head_event_seq`
- `active_checkpoint_id`
- `updated_by_event_seq`
- `updated_at_ms`

这样 feed 仍然能展示完整审计历史，agent restore 只消费当前模型可见 projection。

## 与压缩 checkpoint 的关系

压缩 checkpoint 必须 branch-aware：

- checkpoint 覆盖边界要绑定 `session_id + event_seq/ref`，不能只绑定消息计数。
- fork child 可以继承 parent fork point 之前的 latest checkpoint，但恢复时需要以 fork point 固定的 parent projection 为 base，再 replay child suffix。
- rollback 到某个 checkpoint 之前时，之后创建的 checkpoint 不能继续作为 active checkpoint。
- continuation / executor restore 应该从 `active projection cursor -> latest valid checkpoint -> suffix` 三段式恢复。

## 推荐拆分

当前压缩任务需要先确定这些长期形状，但不必一次实现完整 session tree UI/API。建议：

1. 本任务落地 checkpoint 仓储和 branch-aware projection 的最小字段。
2. 后续独立任务实现完整 session fork / rollback / lineage API 与 UI tree。
3. 如果先做 branch API，本任务至少要保留 checkpoint schema 的 `lineage_node_id` / `fork_point_checkpoint_id` 预留位。
