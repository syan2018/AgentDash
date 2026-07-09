# Context Compaction Projection

## Scope

上下文压缩投影契约适用于平台自有 Pi/native runtime 的结构性 compact、session resume、模型上下文查询和后续 fork / rollback 基线。外部 executor 自行完成的 compact 在缺少 summary、boundary 与 replacement history 时只能证明“外部上下文已经变化”，无法证明 AgentDash 模型投影的替换范围，因此进入 `executor_context_compacted` 遥测事件，而不参与 projection commit。

## Durable Shape

`session_events` 保存真实发生过的事实：`UserInputSubmitted` 用户输入、assistant 输出、工具生命周期、compact lifecycle、failure diagnostic 和 ContextFrame。compact 不改写历史事件，只提交新的模型上下文 projection。

成功结构性 compact 使用三类持久化对象表达可恢复状态：

| Store | 职责 |
| --- | --- |
| `session_compactions` | checkpoint-oriented record，记录 lifecycle item、status、strategy、trigger、phase、source range、first kept pointer、token stats、summary 与 replacement projection metadata |
| `session_projection_segments` | 可恢复投影片段，当前 MVP 至少写入 `summary_chunk`，后续策略可扩展为 `pruned_message`、`tool_result_digest`、`artifact_reference` |
| `session_projection_heads` | 当前 projection kind 的 active cursor，记录 `projection_version`、`head_event_seq` 与 `active_compaction_id` |

PostgreSQL 与 SQLite repository 必须把 compact completed event、compaction record、projection segments 和 projection head 放在同一提交单元中。提交失败时 active projection head 保持原值。

Projection store 的 head key 是 `(session_id, projection_kind)`，segment 顺序唯一性是 `(session_id, projection_kind, projection_version, sort_order)`。同一 session 的模型可见上下文只有一个当前 head；session 树拓扑由 `session_lineage` 表达，因为 lineage 记录的是会话关系，projection store 记录的是某个 session 当前可恢复的模型输入。

结构性 compact 的摘要生成同样通过 `BridgeRequest` 的原生 `AgentMessage` 序列进入 provider bridge：system prompt 表达摘要目标，request messages 只添加摘要任务说明并保留待摘要的原始 User / Assistant / ToolResult / content parts。原因是摘要路径需要复用 Agent 正常请求的 provider adapter 转换边界，让工具调用、工具结果、多模态内容、Context panel 展示与 token 估算保持同一套模型可见口径。

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

当 compact 策略被触发但当前 transcript 没有合法 `MessageRef` cut point 时，runtime 发送 `PlatformEvent::SessionMetaUpdate(key = "context_compaction_noop")`，payload 携带 `noop_reason` 与 compaction provenance。该事件只用于用户命令和运行诊断可观察性，不创建 `session_compactions(status = projection_committed)`，原因是没有 summary、source range 和 replacement projection 时，active projection checkpoint 不能被替换。

内部 `context_compacted` payload 是显式 boundary 契约，必须携带：

| Field | 含义 |
| --- | --- |
| `summary` | 写入 `session_compactions.summary` 与 summary projection segment 的模型可见摘要 |
| `compacted_until_ref` | 被摘要覆盖的最后一条 `MessageRef`，用于精确解析 `source_end_event_seq` |
| `first_kept_ref` | 压缩后保留的第一条 `MessageRef`；压缩到末尾时显式为 `null` |
| `messages_compacted` | 诊断与展示计数，不作为恢复边界 |

后端只接受这些显式边界，因为 `MessageRef` 是 runtime 输入与持久化 transcript 的共同坐标；这样 projection commit 能从同一份 transcript 精确解析 source range，并避免把计数、timeline shape 或外部 executor 私有行为当作恢复事实。

`context_compaction_failed` 是结构化 diagnostic。失败不会生成 `session_compactions(status = projection_committed)`，也不会替换 active projection head。Hook runtime 会记录连续失败次数，达到阈值后暂停后续自动压缩尝试；成功 compact 会复位该计数。

## Scenario: Manual Compact-Only Maintenance Turn

### 1. Scope / Trigger

手动 context compaction 在当前 AgentRun 没有 live running turn 时会启动 compact-only maintenance turn。该 turn 的执行目标只有结构性 compact，因此它的成功边界是 `session_compactions` / `session_projection_segments` / `session_projection_heads` 同步提交完成，而不是 maintenance prompt 被 accepted 或摘要文本生成完成。

### 2. Signatures

```rust
pub enum AgentRunContextCompactionRuntimeLaunchOutcome {
    Launched { turn_id: String },
    Completed { turn_id: String, message: Option<String> },
    NoEligibleMessages { turn_id: String, message: Option<String> },
    Failed { turn_id: String, message: Option<String> },
    NotImplemented { message: String },
}

pub enum CompactionPreflightOutcome {
    NotRequested,
    Noop { reason: String },
    Completed,
    Failed { reason: String },
}
```

`runtime_session_compaction_requests` 继续作为命令级 lifecycle 记录：`requested -> consumed -> completed | noop | failed`。`completed` 必须携带 `completed_compaction_id`、`compacted_until_ref`、`first_kept_ref` 与 result metadata；`noop` 和 `failed` 必须携带稳定 reason。

### 3. Contracts

- compact-only launch source 是 `LaunchSource::ContextCompaction`，无 live runtime 且已有历史事件时必须走 `RepositoryRehydrate(ExecutorState)`，原因是 maintenance turn 要消费 AgentDash 自己的 durable model context 和 `MessageRef` 坐标。
- `no_eligible_messages` 只表示模型上下文已完整 materialize，但 keep-last / token budget / cut point 判断没有可压缩前缀。
- `messages.len() == 0`、`message_refs.len() != messages.len()`、`compacted_until_ref` 缺失、`first_kept_ref` 缺失属于结构性失败，进入 `ContextCompactionFailed` 与 request `failed`。
- command receipt 在 `Launched`、`Completed`、`NoEligibleMessages`、`Failed` 中都保留 maintenance `turn_id`，原因是用户和诊断工具要能从命令结果追到同一条 session lifecycle。
- projection commit 完成后再把 manual request 标记为 `completed`，原因是 request 终态必须代表可恢复 checkpoint 已经落库。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| 已恢复完整 transcript 且 cut point 不存在 | emit `context_compaction_noop`，request `noop(reason=no_eligible_messages)`，不写 projection head |
| 恢复上下文为空 | emit `context_compaction_failed`，request `failed(reason=compaction_context_empty)` |
| `messages` 与 `message_refs` 长度不一致 | emit `context_compaction_failed`，request `failed(reason=compaction_message_ref_len_mismatch)` |
| cut boundary ref 缺失 | emit `context_compaction_failed`，request `failed(reason=compaction_boundary_ref_missing)` |
| first-kept ref 缺失 | emit `context_compaction_failed`，request `failed(reason=compaction_first_kept_ref_missing)` |
| summary provider / projection commit / cancel 失败 | request `failed`，result metadata 保留 reason、lifecycle item id 与 diagnostic details |
| projection checkpoint 成功提交 | request `completed`，写入 compaction id、boundary refs 和 projection metadata |

### 5. Good / Base / Bad Cases

- Good: idle/completed AgentRun 触发 compact-only，runtime-session 冷启动恢复 projected transcript，preflight 判定 eligible，summary 生成后提交 projection checkpoint，manual request 进入 `completed`，command receipt 保留 maintenance `turn_id`。
- Base: 当前 transcript 完整但只有少量消息，preflight 进入 `NoEligibleMessages`，request 为 `noop`，timeline 只出现 noop diagnostic。
- Bad: model context 没有被恢复出任何历史消息时返回 `no_eligible_messages`，因为这会让 command receipt 看起来成功处理了业务 noop，却没有暴露恢复链路的结构性问题。

### 6. Tests Required

- Agent loop eligibility 单元测试覆盖真实 noop、空上下文、message/ref 长度不一致、boundary ref 缺失和 first-kept ref 缺失。
- Compact-only agent loop 测试断言结构性失败会发 `ContextCompactionFailed`，不会发 `ContextCompactionNoop`。
- Runtime-session launch path 测试断言 `LaunchSource::ContextCompaction` 冷启动使用 `RepositoryRehydrate(ExecutorState)`，即使 trace meta 存在 executor follow-up。
- Manual delegate 测试断言 consumed request 的失败会写入 `failed` 和 diagnostic metadata。
- Eventing/projection 测试断言 `context_compacted` projection commit 后 manual request 才进入 `completed`，并带有 compaction id 与 boundary refs。
- AgentRun command 测试断言 compact-only 的 `completed` / `noop` / `failed` 都保留 maintenance `turn_id`。

### 7. Boundary Mismatch / Canonical

#### Boundary Mismatch

```rust
if !should_execute_compaction(messages, refs, params) {
    return NoEligibleMessages;
}
```

#### Canonical

```rust
match evaluate_compaction_eligibility(messages, refs, params) {
    CompactionEligibility::Eligible => execute_compaction(...),
    CompactionEligibility::NoEligibleMessages { .. } => mark_noop("no_eligible_messages"),
    CompactionEligibility::InvalidInput { failure, .. } => mark_failed(failure.reason_code()),
}
```

结构性 eligibility 分类放在 agent loop preflight 边界，原因是 running next-turn manual compact 与 compact-only maintenance turn 必须共享同一套 request lifecycle 语义。

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

用户输入投影以 `UserInputSubmitted.item_id` 作为 message key，并按事件顺序生成模型可见 user message。`submission_kind=prompt` 和 `submission_kind=steer` 可以落在同一个 turn 内但必须保留不同 item id，原因是 resume、fork、rollback 和后续 compact 都需要复原 Codex thread history 的真实 user message 边界。

工具结果投影以持久化后的 bounded `AgentToolResult` / ThreadItem 为 canonical source。
如果工具结果带有 `details.truncation` 或 `lifecycle_path`，projection、continuation、
repository rehydrate 和 compaction summary 只能渲染 preview/ref/status 这类已持久化小文本。
它们不自动读取 `lifecycle://session/tool-results/.../result.txt` 或 terminal log body，原因是
resume 必须恢复模型当时实际看到的 bounded fact，而不是把会话期缓存重新扩写进模型输入。

`AgentContextEnvelope` 内的每条 `AgentInputMessage` 必须携带 provenance：

| Field | 含义 |
| --- | --- |
| `origin` | `event` 表示真实事件投影；`projection` 表示摘要、裁剪或 digest 等派生片段 |
| `synthetic` | 派生模型输入为 `true` |
| `source_event_seq` / `source_range` | 该条模型输入来源的事实事件坐标 |
| `projection_segment_id` | 派生内容对应的 projection segment |
| `active_compaction_id` | envelope 当前使用的 active checkpoint |

产品入口 `GET /agent-runs/{run_id}/agents/{agent_id}/runtime/context/projection` 通过当前 delivery `RuntimeSessionExecutionAnchor` 返回 `model_context` projection view，前端 AgentRun Context panel 用它展示模型当前可见 segments。Runtime trace 诊断读取复用同一 projection 加载逻辑并先解析 anchor 做 Project `Use` 校验。Timeline 继续消费真实事件流，两者不互相替代。

projection view 同时返回 `context_usage` 分析数据，用于上下文查看窗口展示 Claude Code 粒度的主分类与二级详情。分类估算来自 `AgentContextEnvelope` 中的 projection segments 与统一 token estimation helper；provider usage 仍是总量和窗口压力的权威来源。这个拆分让窗口能够解释“当前模型可见内容的构成”，同时避免前端重复实现 message/tool/summary token 估算。

压缩触发统计使用当前 provider-visible context pressure 与 effective window：

```text
context_pressure = current_context_tokens
threshold = effective_context_window - reserve_tokens
```

Anthropic/Claude 类 provider 的当前上下文输入需要把 cache read 与 cache creation input 纳入压力值，因为这些 token 仍然占用本轮模型可见上下文。provider usage 尚未返回时，runtime 可以使用本地 request estimate 作为 pending estimate，使状态提示与压缩判断在两次真实 usage 之间保持连续。

## Lifecycle Recall Surface

Compaction summary 是模型可见的交接文本，但原始意图与工具细节仍以 session events / ThreadItem 为事实源。Lifecycle VFS 因此提供 session 级回看文件面：

| Path | 职责 |
| --- | --- |
| `session/items` | 全量 item 索引，包括用户消息、Agent 消息、reasoning、工具 item 与 context compaction item |
| `session/messages` | 用户消息与 Agent 消息视图，文件名携带 item id、role 与内容预览 |
| `session/tools` | 工具类 ThreadItem 视图，文件内容保留原始 ThreadItem JSON |
| `session/tool-results/{turn_alias}/{body_alias}/metadata.json` | 工具结果裁切 metadata、readable ref、raw trace 与 body 状态 |
| `session/tool-results/{turn_alias}/{body_alias}/result.txt` | 会话期工具结果缓存正文；missing / expired 返回带 readable path 的有界状态文本 |
| `session/terminal/{terminal_alias}.metadata.json` | terminal 输出 metadata 与 body 状态；metadata 保留 raw terminal id |
| `session/terminal/{terminal_alias}.log` | retained terminal log 读取入口；不可用时返回带 `term_###` 的有界状态文本 |
| `session/writes` | 成功写入类工具 item 子集，文件名携带写入目标 |
| `session/summaries` | 每轮 context compaction summary 的标准留档 |

这些文件名直接作为低成本索引提供给 summarizer，原因是后续 Agent 需要先扫目录确定值得回看的原文，再按需读取具体文件；summary 文本只承担交接和引用职责，不替代原始事件审计。文件名使用 readable alias，原因是这些路径会进入模型主上下文和用户可见 recall surface；raw provider / terminal id 只保留在 metadata / trace 中用于诊断。

`session/tool-results` 与 `session/terminal` 的 body path 仍通过 `fs_read` 读取，因此大文件 full-read
防御和 `offset/limit` 分段读取继续生效。Lifecycle search 只索引 metadata / preview，不扫描完整
result/log body。

## Branch Baseline

fork / rollback / lineage 消费 `session_compactions` 的 checkpoint surface 与 `session_projection_heads`，并通过 `session_lineage` 记录跨 session 的父子关系。Projection head 表示“该 session 当前模型可见到哪里”，lineage edge 表示“该 session 从哪里来”；两者分离后，rollback 可以移动当前 head，fork 可以 materialize child initial projection，而不会让 projection store 同时承担 session tree 的职责。
