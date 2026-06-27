# 实现 apply patch 流式预览设计

## 背景

Codex 在模型流式输出 `apply_patch` 参数时，通过增量 parser 将 partial patch 转换为结构化 `FileChange`，再向客户端发送 patch update 事件。AgentDash 当前 `fs_apply_patch` 只有在完整工具参数可解析或工具执行开始后，才会映射成 `fileChange` item。用户在模型生成较长 patch 时看不到“正在编辑哪些文件”。

AgentDash 已有的事实链路是：

```text
AgentEvent::MessageUpdate(ToolCallDelta)
  -> pi_agent::stream_mapper
  -> BackboneEnvelope(ItemStarted / ItemCompleted / delta events)
  -> SessionEvent persistence + NDJSON
  -> frontend session reducer
  -> FileChangeCardBody / DiffCardBody
```

## 设计

### 事件形态

优先复用现有 `BackboneEvent::ItemStarted(ItemStartedNotification)`，item 使用 Codex `ThreadItem::FileChange`，`status = PatchApplyStatus::InProgress`。

这样做的原因：

- `fileChange` 已是 Backbone 中表达文件修改的统一 item。
- 前端已有 `item:{item_id}` upsert 逻辑，可以把重复 `item_started` 当作预览更新。
- 不新增协议变体即可避免 generated TS 和持久化事件类型扩散。

### 映射位置

工具更新分为两条上游：

- `AssistantStreamEvent::ToolCallDelta`：工具输入生成中的更新，来源是模型正在流式输出 tool-call arguments。
- `AgentEvent::ToolExecutionUpdate`：工具执行过程中的更新，来源是工具自身 `on_update` 回调。

两者都应更新同一个工具 item，但语义不同。`fs_apply_patch` 的特殊处理属于输入更新路径：它从 arguments draft 中解析 patch 草稿，并生成结构化 `fileChange` 预览。

流程：

1. `AssistantStreamEvent::ToolCallDelta` 继续维护工具调用状态，确保 tool call 已有稳定 `entry_index`、tool name 与 raw input。
2. 非 `fs_apply_patch` 工具在 `draft` 已是完整 JSON 时，更新同一 `dynamicToolCall` / native tool item 的 arguments。
3. `fs_apply_patch` 从 partial JSON draft 中提取已完成转义的 `patch` 字符串内容，即使整体 JSON 尚未闭合也可尝试预览。
4. 将 patch 内容转换为 `Vec<FileChangeSpec>`。
5. 构造同一 item id 的 `fileChange(status=in_progress)` 并发送 `ItemStarted`。
6. `AgentEvent::ToolExecutionUpdate` 继续映射执行期输出：shell 输出走 `CommandOutputDelta`，普通工具输出走同一 `dynamicToolCall(status=in_progress, contentItems=...)` item。

### Partial JSON 解析

`AssistantStreamEvent::ToolCallDelta` 已携带：

- `delta`: 当前增量。
- `draft`: 当前完整参数草稿。
- `is_parseable`: draft 是否已是完整 JSON。

当 `is_parseable = true` 时，直接从完整 JSON 读取 `patch` 字段。

当 `is_parseable = false` 时，使用窄解析器从 draft 中读取 `"patch":"...` 已出现的字符串内容。解析器只处理 JSON string escape 到 UTF-8 string 的最小语义；遇到未闭合转义时返回当前可用前缀或静默。这样做的原因是预览只服务 UI，不应让 partial JSON 解析错误影响工具调用本身。

### Patch 解析

复用并收敛现有 `parse_apply_patch_specs` 语义：

- 支持 `*** Add File: path`
- 支持 `*** Delete File: path`
- 支持 `*** Update File: path` + `@@` hunk
- 支持 `*** Move to: new_path`

流式预览允许 patch 尚未出现 `*** End Patch`。解析器以当前可完成的 file op 为单位输出可展示内容；当前未完成的 file op 可以在后续 delta 中更新。

### 前端

前端 reducer 当前对同一 `item_id` 的 `item_started` 已执行更新而非追加。若测试证明 fileChange repeated start 可以正确刷新内容，则前端无需新增 UI。必要时补充 reducer 测试来锁定该行为。

### 通用工具更新

输入更新链路：

```text
LLM StreamChunk::ToolCallDelta
  -> AssistantStreamEvent::ToolCallDelta
  -> pi_agent::stream_mapper
  -> ItemStarted(dynamicToolCall/fileChange, status=in_progress, arguments/changes)
  -> frontend reducer 按 item_id 更新同一工具卡片
```

执行更新链路：

```text
AgentTool::execute(..., on_update)
  -> AgentEvent::ToolExecutionUpdate
  -> pi_agent::stream_mapper
  -> ItemStarted(dynamicToolCall, status=in_progress, contentItems=partial output)
  -> frontend reducer 按 item_id 更新同一工具卡片
```

Codex connector 侧还会透传 Codex 原生 `item/mcpToolCall/progress` 为 Backbone `mcp_tool_call_progress`。PiAgent 侧不应把输入预览或进度抽象绑到 MCP，而应要求所有工具输入都能通过 `ToolCallDelta` 刷新，所有能产生执行中间状态的工具通过 `on_update` 上报真实增量；mapper 再按工具类型选择 fileChange、命令输出或 dynamicToolCall 展示。

## 约束

- 不引入新数据库字段或 migration。
- 不从前端推断 patch 状态；前端只消费后端发出的 Backbone item。
- 不把 `apply_patch`、`fs_apply_patch` 和其它工具名混用；本任务只支持当前 `fs_apply_patch`。
- 不改变工具执行安全、审批或 VFS 写入语义；预览不是执行事实。
- 通用工具进度不应假装有准确百分比；没有结构化进度时只展示工具主动上报的真实文本/内容片段。输入预览不应伪造执行输出。

## 风险

- partial JSON 中 `patch` 字段很长，解析器必须只做线性扫描，不做昂贵回溯。
- repeated `ItemStarted` 的语义需要测试锁定，避免未来 reducer 改动导致重复卡片。
- 现有 patch spec parser 对未闭合 patch 不友好，需要为 preview 单独提供宽松解析路径，而执行路径仍保持严格。
- 真实进度能力取决于工具实现是否能上报中间状态；没有底层事件时只能显示“已开始执行”的 in-progress 状态。
