# 压缩摘要锚点与 Lifecycle Session 回溯设计

## Goal

让上下文压缩产物在一段可读 summary 中自然标识关键原始会话位置，而不是引入独立的结构化 Anchor 模型，也不为每条消息塞详细坐标。压缩摘要应像一份带稀疏索引的交接记录：说明做过什么、为什么这么做，并用少量 Lifecycle session 文件名、Agent message 区间或 ThreadItem id 提示后续 Agent 去回看原文。

该能力服务两类使用场景：

- 长会话压缩后，模型仍能知道哪些历史片段值得按需回看。
- Lifecycle 空间中的 session items/messages/tools/writes/summaries 投影能承担“工作过程索引”的角色，而不只是原始事件归档。

## Confirmed Facts

- 当前 compaction 已经是 durable model context projection，而不是直接改写原始 timeline。成功压缩会写入 `session_compactions`、`session_projection_segments`、`session_projection_heads`。
- 当前 compaction projection 只写入一个 `summary_chunk` segment，`content_json` 主要保存 summary 文本。
- `ContextProjector` 会把 `summary_chunk` materialize 成模型上下文，因此最简单的 MVP 是直接增强 summary 文本本身。
- Lifecycle journey projection 已提供 `session/events.json`、`session/turns`、`session/turns/{turn_id}/events.json`、`tool-calls`、`tool-calls/{id}/request.json|result.json|stdout.txt`、`writes` 等虚拟路径；这些路径当前信息密度不足，且 `tool-calls` 过早拆成 request/result/stdout/raw 四类文件。
- 当前 Lifecycle `session/turns` 按 persisted `turn_id` 分组；这个 `turn_id` 来自 session launch / connector accepted turn，不适合作为压缩回看索引主轴。
- `pi_agent` stream mapper 已经为 agent message / reasoning / tool item 生成或传递 `item_id` 与 `entry_index`：agent text 使用 `{turn_id}:{entry_index}:msg`，reasoning 使用 `{turn_id}:{entry_index}:reason`，工具调用进入 `ThreadItem` lifecycle。这个粒度更适合作为 Lifecycle session 文件树和压缩摘要回看索引。
- 当前 tool call projection 能按 tool call id 聚合 request/result/stdout/raw events，但没有被 compaction 摘要明确引用；下一版应直接以原始 ThreadItem 为核心投影单位，让工具目录文件本身就是一条完整 item。
- Backbone 与 cross-layer spec 已明确：内部 compaction 的可信 projection commit 由 `PlatformEvent::SessionMetaUpdate(key = "context_compacted")` 提供 summary、`compacted_until_ref`、`first_kept_ref`。
- 当前 `serialize_messages_for_summary` 把消息重新串成一段 transcript 文本再发给摘要 LLM。这个方式会改变原始 provider 输入形态，难以复用已有消息前缀缓存；更合理的方向是把总结当作旁支 turn，在原始消息序列后追加总结指令。

## Current Module Assessment

### `agentdash-agent` compaction engine

- 现状：负责选择 cut point、生成/更新 summary、替换为 `[CompactionSummary] + tail`。
- 适配点：摘要生成应从“把历史序列化成文本”调整为“复用原始消息作为旁支请求前缀 + 追加总结指令”，并只提供 Lifecycle 高信息量文件列表或稀疏 message 区间作为回看索引。
- 风险：摘要是自由文本，后端不应尝试解析成一等业务对象；索引只需要帮助 Agent 找回原文，不需要精确到每个 event，也不应误用 connector `turn_id`。

### `agentdash-application::session` projection commit

- 现状：`maybe_commit_compaction_projection` 从 `context_compacted` payload 创建一个 `summary_chunk` segment 并更新 projection head。
- 适配点：继续只提交 summary chunk；同时让每轮 compaction summary 可被 Lifecycle `session/summaries` 标准化读取。
- 风险：如果不结构化解析锚点，后端无法校验每个引用是否真实；MVP 通过 prompt 约束和 Lifecycle 文件列表降低风险。

### Lifecycle journey / VFS

- 现状：从 session events 动态生成 turns、tool-calls、writes 等可读虚拟路径。
- 适配点：重做 Lifecycle session 索引面，形成 `session/items`、`session/messages`、`session/tools`、`session/writes`、`session/summaries`。目录项文件名直接包含 `item_id` 与领域摘要信息，例如消息十词预览、工具名、写入文件路径，使文件列表本身就可以作为 LLM 的低成本索引。
- 风险：无需新增 `session/anchors` 这类实体路径；重点是让 Lifecycle 的 ThreadItem / message / tool 索引更可读、更适合被 summary 引用。现有 `session/turns` 的信息量偏低，预研期可以从主推荐路径中移除或降级为调试视图。

### API / frontend

- 现状：`GET /sessions/{id}/context/projection` 返回 materialized model context segments；前端 `SessionProjectionView` 展示 summary segment preview。
- 适配点：短期无需新增 anchor API。UI 只要能展示增强后的 summary；Lifecycle/VFS 负责回看路径。
- 风险：如果未来要在 UI 中点击锚点跳转，再考虑从 markdown 中识别固定格式引用，而不是现在设计独立数据模型。

## Requirements

- 压缩摘要必须在 markdown 文本中包含“原文回看索引”，用少量 Lifecycle 文件名、Agent message 区间或 ThreadItem id 说明值得回看的历史位置。
- summarizer 输入不应给每条 message 增加详细坐标；MVP 直接把 `session/messages`、`session/tools`、`session/writes` 的高信息量文件列表作为索引材料，消息列表可每约 10 条提供一个区间入口。
- 摘要生成应优先作为旁支 turn 运行：复用原始消息列表作为 provider 前缀，只追加总结指令，提升缓存命中与上下文真实性。
- `session/items` 是全量 item 索引，包括用户消息、Agent 消息、reasoning、工具、compaction 等可持久化 item。
- `session/messages` 只列用户消息与 Agent 消息，文件名包含 `item_id` 和约十词内容预览。
- `session/tools` 只列工具类 item，文件内容使用原始 ThreadItem JSON，不再拆成 `request.json/result.json/stdout.txt/raw.json` 作为主路径。
- `session/writes` 是成功写入类工具 item 子集，文件名包含 `item_id`、工具名和写入目标文件路径。
- `session/summaries` 标准化留档每一轮 compaction 产生的 summary，支持后续 Agent 查看压缩历史。
- 规划和实现不得把现有 connector `turn_id` 当作回看主索引；如保留 `turn_id` 字段，应明确它只是外层 launch trace。
- 不新增独立 Anchor 表、Anchor segment 或复杂结构化输出模型。
- 保持已有 raw session events 是事实源；摘要里的锚点只是可读引用，不替代审计 timeline。

## Acceptance Criteria

- [ ] PRD 明确采用“文本锚点目录”而非结构化 Anchor 实体。
- [ ] Design 明确旁支总结方式、summary 文本格式、Lifecycle items/messages/tools/writes/summaries 文件索引的配合方式。
- [ ] Design 明确不新增 projection segment type / 独立表 / anchor API 的 MVP 边界。
- [ ] Implement plan 覆盖摘要 prompt、旁支 summarization 请求、Lifecycle session projection 清洗、summary 留档、测试与 UI 验证。
- [ ] 当前关联模块状态评估已记录，并列出 MVP 与后续扩展边界。

## Out of Scope

- 本任务不先优化真实 tokenizer 计算。
- 本任务不处理外部 executor 缺少 replacement provenance 的 compaction commit。
- 本任务不新增结构化 Anchor 数据模型、Anchor projection segment 或专门 Anchor API。
- 本任务不把所有历史原文自动注入压缩后的 provider context。
- 本任务不重新设计 session lineage / fork / rollback 语义。

## Open Questions

- 无。当前采用 `session/tools` / `session/writes` 文件内容直接输出原始 ThreadItem JSON，索引信息放在文件名与列表 summary 中。

## Notes

- 这属于复杂任务，需要 `design.md` 与 `implement.md` 后再进入实现。
