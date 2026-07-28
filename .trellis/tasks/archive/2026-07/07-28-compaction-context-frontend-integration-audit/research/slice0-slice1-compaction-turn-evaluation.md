# Slice 0/1：原会话 Compaction Turn 评估

## 结论

当前 `BridgeDashCompactor` 的执行模型不成立：

- 它从 durable history 另行重建 `InputAccepted/AgentOutput` 文本；
- 它遗漏 `ToolCall/ToolResult` 与正常 provider materializer 已接受的上下文；
- 它用独立 system prompt 发起一次新的无状态 bridge request；
- 它没有复用正常 Turn 的精确上下文成员、顺序和前缀。

正确模型是：在同一 Agent Session 的精确 provider context 上启动一个正式
Compaction Turn，保留已有 system context 与完整 message prefix，只追加一条 synthetic
compaction instruction。provider 返回内容是 checkpoint candidate，不作为普通用户/助手消息写回
conversation。

这不是“把 200k 上下文复制到另一套 transcript”。当前 provider bridge 没有 server-side
thread/session coordinate，因此本地 provider request 仍需携带模型可见上下文；近期正确优化点是复用
完全相同的上下文物化结果与稳定前缀，让 provider prefix cache 可命中，并删除第二套有损 transcript。
未来若 provider 提供原生 remote compaction/session continuation，可作为 provider capability实现同一
Compaction Turn contract，不改变 owner 状态机。

## 代码证据

### 当前 Native compactor 是独立、有损请求

- `crates/agentdash-integration-native-agent/src/bridge_execution.rs:172-239`
  调用 `effective_conversation` 后直接执行 `LlmBridge::complete`。
- 同文件 `297-362` 只投影用户/助手文本，并另行选择 cut/retained membership。
- `crates/agentdash-agent/src/bridge.rs` 的 `BridgeRequest` 不含 session/thread/
  previous-response coordinate。
- OpenAI Responses bridge使用 `store: false` 并提交完整 input，没有
  `previous_response_id`。

因此不能声称当前 compactor“接续 provider session”；它实际创建了与正常 Turn
materialization分离的 prompt lane。

### 正常 Turn 已有精确 materializer

`crates/agentdash-agent/src/dash/service.rs:2070-2191` 的 `materialize_context` 已按当前
successful compaction checkpoint恢复：

- accepted ContextFrames；
- user/assistant messages；
- structured `ToolCall/ToolResult`；
- retained boundary；
- active structured tools。

它为普通 Turn 排除 active input 时使用末尾 `history.pop()`。Compaction Turn 应复用同一个
materializer核心，但请求完整的当前会话 prefix，不执行普通 Turn 的 active-input排除。

### canonical lifecycle 已有可复用 Turn

- durable history已有 `TurnStarted/TurnCompleted/TurnFailed`；
- Backbone已有 `TurnStarted/TurnCompleted`；
- `ContextCompaction` 已是 canonical item kind；
- 当前 projection只把 `CompactionStarted` 发为 `ItemStarted`，没有对应
  `TurnStarted/TurnCompleted`；
- `CompactionFailed` 还与成功一起映射为 `ItemCompleted`。

因此近期不需要引入第二种 activity owner。Compaction 应是一个正式 Turn，包含一个
`ContextCompaction` item；Turn subtype/phase可从 owner 的 typed state无损投影。

### Codex reference采用同一 SessionTask

`references/codex/codex-rs/core/src/tasks/compact.rs` 将 compaction建模为正式
`SessionTask::Compact`。`references/codex/codex-rs/core/src/compact.rs`：

1. 创建 `ContextCompactionItem`；
2. clone同一 session history；
3. 在末尾追加 compaction prompt；
4. 使用该精确 history执行 compact task；
5. 成功后安装 replacement history并重算 token usage。

关键契约是“同一 session history + 正式 compact task”，不是手工维护另一套 summary transcript。

## Slice 0 必须固定的契约

1. 正常 Turn 当前上下文与 Compaction Turn request的 prefix结构完全一致。
2. prefix保留完整 tool call/result pairing与 accepted ContextFrames。
3. Compaction request只在末尾追加一次 synthetic instruction；不重建 summary messages。
4. Compaction Turn不开放新工具调用，但历史工具交互仍保留在 prefix。
5. synthetic instruction与provider summary candidate不写成普通 conversation messages。
6. 成功只通过 `CompactionApplied + CompactionCompleted` 安装 checkpoint。
7. failed/lost不产生成功 context boundary，旧 context recipe与revision保持不变。
8. canonical顺序为 `TurnStarted -> ItemStarted(ContextCompaction) -> applied/terminal ->
   ItemCompleted(success only) -> TurnCompleted`；失败/lost以带 error 的 Turn terminal结束。
9. retained boundary由同一 history/materialization coordinate决定，不允许第二套 projector拆分
   tool pair。

## Slice 1 最小实现边界

1. 删除 `effective_conversation` 及独立 `BridgeDashCompactor` summary transcript。
2. 在 Dash owner提供 dedicated Compaction Turn primitive，直接消费同一
   `DashProvider` 与精确 `DashCoreContext`。
3. 将 context materializer拆成显式模式：
   - normal Turn：排除已经单独提交的 active input；
   - Compaction Turn：保留当前完整 session prefix。
4. Compaction Turn追加一次 synthetic instruction，强制本轮 tools为空。
5. provider output只作为 summary candidate返回给既有 checkpoint apply路径。
6. Compaction source history与canonical projection补齐正式 Turn lifecycle。
7. failed/lost只提交失败 terminal，不提交成功 item/context boundary。
8. Product context projector至少在 Slice 1改为只接受明确成功的 applied checkpoint；
   基于checkpoint membership的完整 query仍由后续 Slice负责。

## 非 Slice 0/1 内容

- 不平行实现相邻任务拥有的统一 Runtime connection/view。
- 不先做前端 loading、Context inspector或完整 checkpoint DTO。
- 不为每种 provider建立 transcript fallback。
- 不声称当前 bridge具备不存在的 server-side session continuation。
- 不在 Compaction Turn中运行普通工具 loop或持久化 synthetic conversation records。
