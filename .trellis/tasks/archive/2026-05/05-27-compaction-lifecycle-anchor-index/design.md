# 压缩摘要锚点与 Lifecycle Session 回溯设计

## Design Goal

把 compaction summary 升级为“带稀疏原文索引的可读交接摘要”。不引入结构化 Anchor 实体，不新增 projection segment type，不给每条 message 注入详细坐标；让摘要文本引用 Lifecycle Session 中高信息密度的 items/messages/tools/writes/summaries 文件列表。

## Proposed Architecture

### 1. Summary Text As The Index

压缩结果仍然只有一个 `summary_chunk`。summary markdown 中新增固定章节：

```markdown
## 当前状态

...

## 关键决策与原因

...

## 原文回看索引

- `session/messages/0010__u_...__结构化_anchor_是过度设计.md`
  - 主题：用户明确否定结构化 Anchor。
  - 为什么值得回看：这里保留了产品意图，summary 不应替代原话。

- `session/writes/0042__fs_apply_patch__trellis_tasks_..._design_md.json`
  - 主题：规划材料被改成文本锚点方案。
  - 为什么值得回看：这里有实际修改依据。

## 未完成事项

...
```

这个章节是给后续 Agent 读的目录，不是后端要解析的业务对象。

### 2. Summarization As A Side Branch Turn

当前 `serialize_messages_for_summary` 把历史消息重新串成 transcript 文本，这不适合作为长期方向：

- 它会把 provider 已经见过的消息前缀变成一段全新的文本，降低缓存命中机会。
- 它丢掉了原始 message / tool call 结构，只保留人工拼接格式。
- 如果再给每条 message 加坐标，会进一步膨胀摘要请求。

推荐方向：把总结当作旁支 turn。摘要请求复用被压缩的原始 `AgentMessage` 列表作为 provider 前缀，只在末尾追加一条总结指令消息：

```text
<原始 messages_to_summarize，保持 provider message shape>

[User]: 请总结以上历史，输出交接摘要，并只能引用下面 Lifecycle 文件列表中出现过的文件名、item id 或 message 区间...
```

这样摘要请求更像“在同一历史上开启一个临时分支问模型要交接结果”，而不是把历史重新包装成一份全新文档。

### 3. Lifecycle File Names As The Low-Cost Index

Lifecycle session 的文件名直接承担索引职责。summary prompt 不需要携带详细坐标，只需要把目录列表或稀疏目录窗口交给 summarizer：

```text
可用于回看的 Lifecycle session 索引：
- session/messages/0001__u_01K...__我们希望压缩支持_lifecycle_回看.md
- session/messages/0010__a_t123_9_msg__我会检查_compaction_和_lifecycle.md
- session/tools/0014__t123_12_tool__fs_read__crates_agentdash_agent_src_compaction_mod_rs.json
- session/writes/0018__t123_18_tool__fs_apply_patch__trellis_tasks_05_27_compaction_lifecycle_anchor_index_design_md.json
```

文件名约定：

- 统一以稳定 ordinal 和 `item_id` 开头，便于排序和精确回看。
- messages 文件名携带约十词内容预览。
- tools 文件名携带工具名和主要目标。
- writes 文件名携带工具名和写入文件路径。
- 文件名只作为可读索引；完整内容放在对应文件内。

真正的详细定位交给 Lifecycle 的 item/message/tool 文件。现有 persisted `turn_id` 是外层 connector turn，不应作为这里的主索引来源。

### 4. Prompt Contract

默认摘要 prompt 从“只写摘要”改成“写交接摘要 + 回看索引”：

- 必须保留用户目标、关键约束、已完成工作、关键决策、未完成事项。
- 必须列出 3-8 个最值得回看的原文文件或文件区间。
- 每个回看项只使用 Lifecycle 索引列表中出现过的文件名、item id 或 message 区间，不能编造精确坐标。
- 每个回看项必须说明“为什么摘要不足以替代这段原文”。
- 对工具输出、错误日志、文件修改，优先引用 `session/tools` 或 `session/writes` 中的 item 文件。

二次压缩 prompt 同样保留并更新“原文回看索引”，删除过时项。

### 5. Lifecycle Session Projection

清洗现有 Lifecycle VFS，而不是新增 anchor 路径。这里的主索引跟持久化 ThreadItem / agent/user message 走，不跟当前 `trace.turn_id` 走：

- `session/items`：全量 item index，包括用户消息、Agent 消息、reasoning、工具 item、context compaction item。列表项文件名包含 `ordinal + item_id + kind + preview`。
- `session/messages`：用户消息与 Agent 消息视图。文件名包含 `ordinal + item_id + role + 十词预览`，文件内容为 markdown 原文与必要 metadata。
- `session/tools`：工具类 ThreadItem 视图。文件名包含 `ordinal + item_id + tool_name + target`，文件内容直接使用原始 ThreadItem JSON，不把主路径拆成 request/result/stdout/raw 四个文件。
- `session/writes`：成功写入类工具 item 子集。文件名包含 `ordinal + item_id + tool_name + written_path`，文件内容同样保留原始 ThreadItem JSON。
- `session/summaries`：每一轮 compaction summary 留档。文件名包含 `ordinal + compaction_id + compacted_until_ref`，文件内容为该轮 summary markdown 与必要 metadata。
- `nodes/{step_key}/session/...` 同步提供指定 node 的相同 projection。

`SessionItemSummary` 仅作为索引列表 JSON 使用，文件内容仍以原始 item 为准：

- `first_event_seq`
- `last_event_seq`
- `item_index`
- `item_id`
- `item_kind`
- `preview`
- `path`
- `status`

这样 summary 中引用某个文件名、`message 20-29` 或某个开头片段后，Agent 可以先读 Lifecycle 文件列表定位，再读相关 item 原文。

当前实现注意点：

- `crates/agentdash-application/src/session/launch/orchestrator.rs` 为一次 launch 生成 `turn_id`。
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs` 将同一个 `turn_id` 套到该 connector run 的所有 Backbone envelopes。
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` 会为 agent text/reasoning 生成稳定 `item_id`，并为工具调用发出 `ItemStarted/ItemCompleted`。
- 因此 MVP 可以基于 persisted Backbone events 派生 session item index：用户消息、message delta item、reasoning item、tool ThreadItem、context compaction item 都进入 `session/items`；不能直接复用 `turn_id` 分组。

### 6. Projection Commit And Summary Archive

`maybe_commit_compaction_projection` 不需要新增 segment。继续写：

- `session_compactions.summary`
- `summary_chunk.content_json.content`
- projection head

同时新增 Lifecycle 读取口径：`session/summaries` 从 `session_compactions` 或 projection commit records 读取每轮压缩 summary。`diagnostics_json` 记录 summary prompt version，例如：

```json
{
  "summary_format": "markdown_with_recall_index_v1"
}
```

这里要区分两个概念：

- `session/summary` 是 Lifecycle node 级别的工作摘要。
- `session/summaries` 是每轮 context compaction 的 summary archive。

### 7. API / Frontend

MVP 不新增 anchor API。

前端 projection panel 自然展示增强后的 summary preview；如果后续要更好展示，可以只优化 markdown 展示或在 Lifecycle view 中强化 messages/tools/writes/summaries 列表。

## Current Module Fit

- `summary_chunk` 已是模型上下文的一等投影片段，直接增强文本最贴合现有架构。
- Lifecycle VFS 已有外层 turn 和 tool call 回看路径；本任务需要把主索引清洗为 items/messages/tools/writes/summaries，避免和 connector `turn_id` 混淆，并让文件树跟持久化 ThreadItem 对齐。
- Tool call projection 当前能反查 request/result/stdout，但主路径信息密度不足；下一版应让工具文件直接承载原始 ThreadItem，并把工具名/目标写进文件名。
- 不新增 segment type 可以避免数据库迁移、contracts 扩散和 UI 概念膨胀。
- 旁支 turn 摘要方式比 transcript serialization 更接近真实历史输入，也更有机会命中 provider 前缀缓存。

## Trade-Offs

- 文本锚点简单、上下文友好、实现成本低；缺点是后端不能强校验每个引用。
- 结构化 Anchor 可查询性强，但当前阶段过度设计，会引入不必要的数据模型和 UI/API 扩散。
- 高信息量文件名很省 token，并且能直接把目录列表作为 summary 输入；定位粒度不如逐 event 坐标，但符合本阶段“帮助回看”而非“可机器校验”的目标。
- 旁支 turn 需要 bridge 支持用原始消息列表发一次独立 summary request；如果某 provider 对 tool history 有严格约束，需要沿用既有转换器保证消息合法。

## Recommended MVP

1. 保持单一 `summary_chunk`。
2. 摘要生成改为旁支 turn：原始消息列表 + 末尾总结指令。
3. 只附加高信息量 Lifecycle 文件列表或稀疏窗口：messages 可每 10 条一个窗口，tools/writes 直接列关键文件名。
4. 更新 summary prompt，要求 markdown 中包含“原文回看索引”章节。
5. 清洗 Lifecycle session index；新增 `session/items`、`session/messages`、`session/tools`、`session/writes`、`session/summaries` 及对应 `nodes/{step}/session/...` 路径。
6. 不新增 Anchor entity、Anchor segment、Anchor API。
