# 实现 apply patch 流式预览

## Goal

当 PiAgent 在流式输出 `fs_apply_patch` 工具参数时，Session 时间线应在工具真正执行前展示正在生成的文件变更预览。用户可以提前看到将要新增、修改、删除或重命名哪些文件，以及当前已生成的 diff 内容。

## Requirements

- 仅覆盖 AgentDash 当前写文件工具 `fs_apply_patch`，不引入其它 apply patch 名称或兼容路径。
- 预览必须复用 Backbone `fileChange` / 现有前端 FileChange 卡片语义，不新增一套独立 UI 事实源。
- 预览来源是 assistant tool call arguments 的流式 delta；工具执行开始和执行完成继续使用现有 `ToolExecutionStart` / `ToolExecutionEnd` 映射覆盖终态。
- 解析逻辑必须尊重本项目 `fs_apply_patch` 的 mount URI 路径格式，保留 `mount_id://relative/path` 展示。
- 解析失败或尚未形成可用 patch 时不产生错误卡片，也不影响普通工具调用流。
- 不需要数据库 migration；事件继续通过 Session Backbone envelope 持久化与 NDJSON 推送。
- 不手动维护 generated TypeScript 文件，除非引入新的 Rust protocol type；本任务优先复用现有 `ItemStarted(fileChange)` 事件。
- 同步梳理通用工具输入/执行进度链路：`ToolCallDelta` 是工具输入生成阶段的统一更新源，外部工具在参数 draft 可解析时应刷新输入预览；`ToolExecutionUpdate` 是工具开始执行后的输出/进度更新源。`fs_apply_patch` 只是在输入更新阶段特化解析 patch 草稿，并用 Codex 既有 `fileChange` 语义展示。

## Acceptance Criteria

- [ ] `fs_apply_patch` 的 `ToolCallDelta` 在 draft 中形成有效 patch 片段后，会产出同一 `item_id` 的 `ItemStarted(fileChange, status=in_progress)` 预览事件。
- [ ] 同一工具调用的后续 patch delta 更新同一个 fileChange 条目，而不是创建重复卡片。
- [ ] patch 未闭合或暂不可解析时保持静默；最终 `ToolCallEnd` / `ToolExecutionStart` 仍可通过完整参数生成预览或执行态卡片。
- [ ] 非 `fs_apply_patch` 工具调用不受影响。
- [ ] 前端 session reducer 能把 repeated `item_started` 更新到同一个条目，并保留现有 fileChange 渲染。
- [ ] 覆盖 executor stream mapper 单元测试，必要时补前端 reducer 测试。
- [ ] 明确记录通用工具更新方案：`ToolCallDelta` 更新工具输入预览，`ToolExecutionUpdate` 更新工具执行输出；`fs_apply_patch` 的输入草稿走结构化 fileChange，未知工具输入先复用 dynamicToolCall，后续需要按工具输出质量补专用 renderer。

## Notes

- 参考 Codex 的 `ApplyPatchArgumentDiffConsumer` / `StreamingPatchParser` 方案，但 AgentDash 的 `fs_apply_patch` 是 JSON function tool，patch 文本位于 `{"patch":"..."}` 字段内，因此实现点应放在 PiAgent stream mapper 解析 tool-call draft 的位置。
- Codex bridge 已会透传 Codex 既有 `fileChange` / `mcpToolCall` 增量事件；PiAgent 侧应区分工具输入更新和执行更新，不要把通用输入预览能力绑定到 MCP 这一类工具。
