# 实施计划

## Checklist

- [ ] 更新 compaction summary prompt，新增固定 markdown 章节“原文回看索引”。
- [ ] 更新 update summary prompt，要求保留、更新、删除过时的回看索引项。
- [ ] 将摘要请求从 transcript serialization 调整为旁支 turn：复用原始 `messages_to_summarize`，只追加一条总结指令消息。
- [ ] 构建高信息量 Lifecycle 文件列表索引，messages 默认每 10 条给一个窗口，tools/writes 直接列关键文件名。
- [ ] 移除 `serialize_messages_for_summary` 作为主总结路径，旁支 summary request 直接复用原始 `messages_to_summarize`。
- [ ] 新增 Lifecycle session item projection：从 persisted Backbone events 派生用户消息、agent message item、reasoning item、tool ThreadItem、context compaction item。
- [ ] 新增或重做 Lifecycle VFS 路径：`session/items`、`session/messages`、`session/tools`、`session/writes`、`session/summaries`，以及对应 `nodes/{step_key}/session/...`。
- [ ] `session/items` 作为全量 item catalog，文件名包含 ordinal、item id、item kind、短预览。
- [ ] `session/messages` 只包含用户消息和 Agent 消息，文件名包含 item id、role、十词预览，文件内容为 markdown 原文与必要 metadata。
- [ ] `session/tools` 文件内容直接输出原始 ThreadItem JSON，文件名包含 item id、工具名和目标；不把 request/result/stdout/raw 四个拆分文件作为主路径。
- [ ] `session/writes` 仅包含成功写入类工具 item，文件名包含 item id、工具名和写入文件路径。
- [ ] `session/summaries` 标准化列出每轮 compaction summary，并能读取单轮 summary markdown。
- [ ] 降级或移除当前信息量不足的 `session/turns` 主索引语义，避免把 `turn_id` 当作回看主索引。
- [ ] 更新 Lifecycle VFS 相关测试，覆盖 items/messages/tools/writes/summaries 和 `nodes/{step_key}/session/...` 的索引与文件读取。
- [ ] 更新 compaction tests，验证摘要请求复用原始消息前缀，只追加总结指令和 Lifecycle 文件列表索引。
- [ ] 如需要，给 compaction metadata diagnostics 增加 `summary_format = markdown_with_recall_index_v1`。
- [ ] 更新 backend session compaction projection spec，说明 summary 文本锚点与 Lifecycle 文件索引协作的原因。

## Validation Commands

- `cargo test -p agentdash-agent compaction`
- `cargo test -p agentdash-application workflow::lifecycle::journey`
- `cargo test -p agentdash-application session::`
- `pnpm --filter @agentdash/app-web test`

## Risky Areas

- compaction engine 当前摘要路径是 serialization，需要改成使用原始 messages 构造旁支 request。
- 摘要文本引用区间无法被后端完全校验，prompt 必须明确“只能引用 Lifecycle 文件列表中出现过的文件名、item id 或 message 区间”。
- 现有 `turn_id` 是外层 launch/connector turn，若误用会把多个 ThreadItem 合并成一组。
- Lifecycle item summary 增强要保持输出可读，避免把完整 events 又塞回索引列表；完整原文放到对应 item 文件中。
- 如果后续 UI 想把文本锚点做成可点击链接，再考虑 markdown link 格式，不在 MVP 中提前建模。

## Cleanup Decisions

- 以 `session/items`、`session/messages`、`session/tools`、`session/writes`、`session/summaries` 作为新的主索引面。
- 当前 `tool-calls/{id}/request.json|result.json|stdout.txt|raw.json` 不是目标形态；实现时可以直接替换为工具 ThreadItem 文件。
- 当前 `session/turns` 只保留为外层 trace 调试信息或从主目录提示中移除。
- 当前 node `session/summary` 与 compaction summary 语义不同；compaction summary 统一进入 `session/summaries`。
