# PiAgent 可见 ID 统一降噪

## Goal

把 PiAgent 会话中暴露给用户、模型、session recall、lifecycle VFS 和前端工具卡片的运行时引用统一收束为短、可读、稳定的 alias。可见引用不再直接展示 provider 原始 `tool_call_id`、时间戳式 `turn_id`、长 terminal id 或其它内部 trace 串；原始 id 继续保留在结构化 metadata / trace 中服务调试。

## Context Audit

本规划已补读以下上下文后修订：

- 前置任务：`.trellis/tasks/archive/2026-06/06-23-piagent-large-output-lifecycle-closeout/{prd,design,implement}.md`
- 后端规范：`.trellis/spec/backend/session/pi-agent-streaming.md`、`.trellis/spec/backend/session/context-compaction-projection.md`、`.trellis/spec/backend/vfs/architecture.md`
- 跨层规范：`.trellis/spec/cross-layer/backbone-protocol.md`
- 前端规范：`.trellis/spec/frontend/type-safety.md`、`.trellis/spec/frontend/hook-guidelines.md`
- 思考指南：`.trellis/spec/guides/cross-layer-thinking-guide.md`、`.trellis/spec/guides/code-reuse-thinking-guide.md`
- 代码入口：AgentLoop/tool_result/tool_call、PiAgent connector/stream_mapper、SessionToolResultCache、lifecycle provider、journey surface、continuation、frontend bounded output parser 和工具/命令卡片展示组件。

## Confirmed Facts

- 上一个收口任务已经解决大输出链路闭环：bounded preview、共享 `SessionToolResultCache`、ThreadItem id 和 lifecycle VFS 使用同一个 stable item id。
- 当前 stable item id 是 `{turn_id}:{tool_call_id}`；这个选择是为了让 producer 边界先闭合链路，不是最终用户可见命名要求。
- 当前 `turn_id` 由 session launch 按毫秒时间戳生成，形如 `t1782186042522`。
- 当前 provider `tool_call_id` 可能包含长随机串、竖线、哈希片段，已经进入 bounded preview、`details.lifecycle_path`、ThreadItem id、continuation summary 和 frontend card 文本。
- lifecycle recall surface 不只有 `provider_lifecycle.rs`，还包括 `lifecycle/surface/journey/mod.rs` 与 `session_items.rs`；这些路径也按 `session/tool-results/{item_id}` 和 `session/terminal/{terminal_id}` 组织可读文件面。
- frontend `boundedOutput.ts` 只解析 lifecycle path；`ToolOutputContentViewer.tsx` 与 `CommandExecutionCardBody.tsx` 会把 path 展示出来；`useSessionFeed` 依赖 bounded/truncation marker 保持单卡可见。
- context projection / continuation 明确只恢复 persisted bounded fact，不读取 full body；因此可见 path 降噪必须发生在 producer/ref 层，而不是只在 UI 层美化。
- `SessionMessageCard.tsx` 当前工作区有非本任务产生的换行相关修改；它有助于长文本不撑破 UI，但不能替代 ID 降噪。

## Requirements

- 建立 session-scoped readable alias 机制，统一生成并复用 `turn_001`、`tool_001`、`cmd_001`、`term_001` 等 `前缀_ID` 风格可见 ID。
- PiAgent tool result full body 引用使用短分段路径：
  `lifecycle://session/tool-results/turn_001/tool_001/result.txt`。
- Tool / command ThreadItem id 使用与 lifecycle path 同源的短 id，例如 `turn_001:tool_001` 或 `turn_001:cmd_001`。
- terminal lifecycle 引用和 status 文本使用短 alias，例如 `lifecycle://session/terminal/term_001.log`，metadata 保留 raw terminal id。
- lifecycle VFS 与 journey surface 的 tool-results 文件面切到短分段路径：
  `session/tool-results/turn_001/tool_001/{metadata.json,result.txt}`。
- `SessionToolResultCache` 使用 readable ref 作为 body lookup key，同时 metadata 保留 raw `turn_id`、raw `tool_call_id`、tool/terminal 类型、可见 alias 和 lifecycle path。
- continuation、projected transcript、cache missing / expired status message 和前端 bounded card 的默认可见文本只展示短 alias/path。
- raw provider id、raw turn id、raw terminal id、source trace 继续保留在结构化 metadata / trace / 日志中，供排查和事件索引使用。
- 前端不能自行发明 alias；前端只消费后端产出的短 path / metadata，并以短引用为默认展示。
- 同一 session 内 alias 分配稳定且单调递增：同一 raw turn/tool/terminal 重复出现时得到同一个 alias，新对象按出现顺序分配下一个编号，编号使用固定宽度三位十进制，超过 `999` 后自然扩展为更多位。

## Acceptance Criteria

- [ ] 大工具结果 bounded preview 中的 `lifecycle_path` 形如 `lifecycle://session/tool-results/turn_001/tool_001/result.txt`，不包含 `call_...` provider id、哈希片段或时间戳式 turn id。
- [ ] shell command / commandExecution 的 bounded output 使用短 command/tool alias，ThreadItem id 与 lifecycle path 同源。
- [ ] terminal metadata / log path 和 missing status 使用 `term_001` 风格 alias，默认可见文本不直接展示 raw terminal id。
- [ ] lifecycle VFS 能列出并读取 `session/tool-results/turn_001/tool_001/metadata.json` 与 `result.txt`。
- [ ] journey surface 的 `session/tool-results`、`session/items`、`session/tools` 文件名和内容引用使用 readable alias；raw trace 只在 metadata/detail 字段中出现。
- [ ] `SessionToolResultCache` 能通过 readable ref 读取完整结果；metadata 能追溯 raw `turn_id` 与 raw `tool_call_id`。
- [ ] continuation / projected transcript 对大输出引用使用短 path，不把 raw id 放进模型主上下文。
- [ ] 前端 bounded output 解析、工具结果卡片和命令卡片展示短引用；长 raw id 不作为默认 UI 文本出现。
- [ ] 后端测试覆盖 alias 分配稳定性、ThreadItem id/path 一致性、cache body round-trip、lifecycle VFS 分段路径、continuation summary 降噪。
- [ ] 前端测试覆盖短 path 解析和 bounded card 展示，不依赖旧扁平 `{turn_id}:{tool_call_id}` path。
- [ ] 更新 `.trellis/spec/backend/session/pi-agent-streaming.md`、`.trellis/spec/backend/session/context-compaction-projection.md`、`.trellis/spec/cross-layer/backbone-protocol.md`，固化 readable alias 与 raw trace 的职责边界。

## Deliverable Slices

1. Tool result / command result readable ref：实现 `turn_001` + `tool_001/cmd_001`，打通 producer、cache、Backbone、VFS、continuation、frontend 展示。
2. Terminal readable ref：实现 `term_001`，覆盖 terminal lifecycle metadata/log path 和 status message。
3. Session recall surface clean-up：清理 `session/items`、`session/tools`、summary/status 文本中默认可见的 raw id。

这些切片共享同一 alias registry；实现可以按切片提交，但验收以整体默认可见面降噪为准。

## Out Of Scope

- 不做旧 path 兼容和迁移；项目未上线，直接收束到正确形态。
- 不把 raw trace 从系统删除；只是不作为默认可见引用。
- 不批量改写历史 archive task、journal 或已存在的旧文本记录。
- 不把完整大输出 body 持久化进 `SessionEvent` 或 projection。

## Decisions

- terminal readable ref 与 tool result readable ref 一起纳入本任务，避免 session recall 可见面继续泄漏 raw terminal id。
- alias 命名采用 `前缀_ID` 风格，例如 `turn_001`、`tool_001`、`cmd_001`、`term_001`。下划线让前缀和序号边界更清楚，路径和 ThreadItem id 连续阅读时更不拥挤。
