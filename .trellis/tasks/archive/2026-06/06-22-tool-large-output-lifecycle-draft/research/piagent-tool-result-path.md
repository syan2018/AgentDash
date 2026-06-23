# Research: PiAgent tool result path

- Query: PiAgent 工具调用结果路径与 result guard 插入点
- Scope: internal
- Date: 2026-06-22

## Findings

### 1. 数据流图 / 步骤

PiAgent native loop 中 `AgentToolResult` 的主路径如下：

```text
Session tool assembly
  -> Vec<DynAgentTool>
  -> PiAgentConnector::prompt
  -> Agent::set_tools / Agent::prompt
  -> agent_loop::stream_assistant_response
  -> provider bridge returns Assistant tool_calls
  -> execute_tool_calls
  -> AgentTool::execute(...)
  -> AgentToolResult
  -> finalize_executed_tool_call(after_tool_call / delegate)
  -> emit_tool_call_outcome
      -> AgentEvent::ToolExecutionEnd(result as JSON)
      -> AgentMessage::ToolResult(content/details/is_error)
      -> AgentEvent::MessageStart/MessageEnd(ToolResult)
  -> agent_loop pushes ToolResult into context.messages
  -> next provider request sees ToolResult in BridgeRequest.messages
  -> Pi stream_mapper maps ToolExecutionEnd to BackboneEvent::ItemCompleted
  -> SessionTurnProcessor persists BackboneEnvelope as PersistedSessionEvent
```

关键分叉点是 `emit_tool_call_outcome` / `emit_tool_result_message`：

- `ToolExecutionEnd` 使用 `serde_json::to_value(result)` 发事件，进入 Backbone `ItemCompleted` 路径；见 `crates/agentdash-agent/src/agent_loop/tool_call.rs:621`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:623`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:626`。
- 同一个 `AgentToolResult` 随后被构造成 `AgentMessage::ToolResult`，并发 `MessageStart/MessageEnd`；见 `crates/agentdash-agent/src/agent_loop/tool_call.rs:646`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:650`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:655`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:662`。
- `agent_loop` 再把返回的 `AgentMessage::ToolResult` push 到 `context.messages` 和 `new_messages`，成为后续模型上下文；见 `crates/agentdash-agent/src/agent_loop.rs:295`、`crates/agentdash-agent/src/agent_loop.rs:306`、`crates/agentdash-agent/src/agent_loop.rs:307`、`crates/agentdash-agent/src/agent_loop.rs:309`。

模型上下文路径：

- 下一次 provider 请求前，`stream_assistant_response` 从 `context.messages` 生成 `messages_for_llm`，再构建 `BridgeRequest { messages, tools }`；见 `crates/agentdash-agent/src/agent_loop/streaming.rs:117`、`crates/agentdash-agent/src/agent_loop/streaming.rs:137`、`crates/agentdash-agent/src/agent_loop/streaming.rs:142`、`crates/agentdash-agent/src/agent_loop/streaming.rs:144`。
- OpenAI Responses bridge 把 `AgentMessage::ToolResult.content` 直接转成 `function_call_output.output`；见 `crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_responses_common.rs:168`、`crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_responses_common.rs:175`、`crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_responses_common.rs:176`。
- `responses_tool_result_output` 对 text 结果会拼接全部文本；见 `crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_content.rs:60`、`crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_content.rs:65`、`crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_content.rs:76`。

Backbone / SessionEvent 路径：

- `PiAgentConnector::prompt` 调用 `agent.prompt(...)` 获取 `event_rx`，然后对每个 `AgentEvent` 调用 `convert_event_to_envelopes_with_runtime_context`；见 `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:788`、`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:817`、`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:834`、`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:872`。
- `ToolExecutionEnd` 被映射为 `BackboneEvent::ItemCompleted`。非 shell 工具会把 `AgentToolResult` decode 成 DynamicToolCall `content_items`；见 `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1025`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1071`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1078`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1081`。
- shell 工具特殊：`ToolExecutionEnd` 从 `result.content[0].text` 读 `aggregated_output` 和 exit code，再构造 shell item；见 `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1040`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1057`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1069`。
- `SessionTurnProcessor` 对 connector stream 产出的 `BackboneEnvelope` 执行 `on_event -> persist`；见 `crates/agentdash-application/src/session/turn_processor.rs:20`、`crates/agentdash-application/src/session/turn_processor.rs:95`、`crates/agentdash-application/src/session/turn_processor.rs:188`、`crates/agentdash-application/src/session/turn_processor.rs:193`。
- `SessionEventingService` 最终调用 `SessionEventStore.append_event` 保存 envelope，并推进 projection head；见 `crates/agentdash-application/src/session/eventing.rs:164`、`crates/agentdash-application/src/session/eventing.rs:167`、`crates/agentdash-application/src/session/eventing.rs:169`。
- Postgres append 会把整条 `BackboneEnvelope` 序列化进 `notification_json`；见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:347`、`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:359`。所以只在持久化层截断已经太晚：模型上下文和前端 stream 可能已经拿到大结果。

### 2. 关键文件和函数

工具结果基础类型：

- `crates/agentdash-agent-types/src/runtime/tool.rs` — `AgentToolResult { content, is_error, details }` 是工具执行结果合同；见 `crates/agentdash-agent-types/src/runtime/tool.rs:24`、`crates/agentdash-agent-types/src/runtime/tool.rs:26`、`crates/agentdash-agent-types/src/runtime/tool.rs:27`、`crates/agentdash-agent-types/src/runtime/tool.rs:30`。`AgentTool::execute` 直接返回该类型；见 `crates/agentdash-agent-types/src/runtime/tool.rs:64`、`crates/agentdash-agent-types/src/runtime/tool.rs:70`。
- `crates/agentdash-agent-types/src/model/message.rs` — `AgentMessage::ToolResult` 是模型上下文中的工具结果消息；见 `crates/agentdash-agent-types/src/model/message.rs:93`、`crates/agentdash-agent-types/src/model/message.rs:99`、`crates/agentdash-agent-types/src/model/message.rs:101`、`crates/agentdash-agent-types/src/model/message.rs:103`。`tool_result_full` 保留 `call_id/tool_name/content/details/is_error`；见 `crates/agentdash-agent-types/src/model/message.rs:194`、`crates/agentdash-agent-types/src/model/message.rs:202`、`crates/agentdash-agent-types/src/model/message.rs:206`。
- `crates/agentdash-agent-types/src/model/content.rs` — `ContentPart` 支持 `Text/Image/Reasoning`；见 `crates/agentdash-agent-types/src/model/content.rs:7`、`crates/agentdash-agent-types/src/model/content.rs:8`、`crates/agentdash-agent-types/src/model/content.rs:11`。

Agent loop：

- `crates/agentdash-agent/src/agent_loop/tool_call.rs` — 工具执行、finalize、事件 emit 和 ToolResult message 构造的核心文件。
- `execute_prepared_tool_call_inner` 调用 `tool.execute(...)` 并得到 `AgentToolResult`；见 `crates/agentdash-agent/src/agent_loop/tool_call.rs:492`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:493`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:496`。
- `finalize_executed_tool_call` 是现有最后一个统一改写机会：runtime delegate 和 `after_tool_call` 都可以覆盖 `content/details/is_error`；见 `crates/agentdash-agent/src/agent_loop/tool_call.rs:510`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:519`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:532`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:555`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:577`。
- `emit_tool_call_outcome` 是事件与模型消息的共同出口；见 `crates/agentdash-agent/src/agent_loop/tool_call.rs:615`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:621`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:638`。

Pi connector / Backbone:

- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs` — 把 `AgentEvent` 转成 `ExecutionStream<BackboneEnvelope>`。
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` — `ToolExecutionStart` -> `ItemStarted`，`ToolExecutionEnd` -> `ItemCompleted`，`ToolExecutionUpdate` -> `CommandOutputDelta` 或 in-progress item；见 `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:868`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:908`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1025`。
- `decode_tool_result_to_content_items` 将 `AgentToolResult.content` 复制为 Codex DynamicToolCall output；见 `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1127`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1130`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1135`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1138`。

工具装配 / runtime gateway / workspace module：

- `crates/agentdash-application/src/session/tool_assembly.rs` — `assemble_tool_surface_for_execution_context` 同源装配 `DynAgentTool` 和模型可见 schema；见 `crates/agentdash-application/src/session/tool_assembly.rs:17`、`crates/agentdash-application/src/session/tool_assembly.rs:23`、`crates/agentdash-application/src/session/tool_assembly.rs:29`、`crates/agentdash-application/src/session/tool_assembly.rs:72`。
- `crates/agentdash-application/src/runtime_gateway/tool_adapter.rs` — RuntimeActionToolAdapter 将 Gateway output 转成 `AgentToolResult`，但只是一个工具来源，不覆盖所有工具；见 `crates/agentdash-application/src/runtime_gateway/tool_adapter.rs:84`、`crates/agentdash-application/src/runtime_gateway/tool_adapter.rs:106`、`crates/agentdash-application/src/runtime_gateway/tool_adapter.rs:115`、`crates/agentdash-application/src/runtime_gateway/tool_adapter.rs:123`。
- `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs` — workspace module 工具只在 capability 允许时注入；见 `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:143`、`crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:148`、`crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:166`、`crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:213`。
- `crates/agentdash-application/src/workspace_module/tools.rs` — 多个 workspace module 工具直接返回 `AgentToolResult`，有些把 runtime output pretty JSON 直接作为 text；见 `crates/agentdash-application/src/workspace_module/tools.rs:224`、`crates/agentdash-application/src/workspace_module/tools.rs:545`、`crates/agentdash-application/src/workspace_module/tools.rs:567`、`crates/agentdash-application/src/workspace_module/tools.rs:569`、`crates/agentdash-application/src/workspace_module/tools.rs:814`、`crates/agentdash-application/src/workspace_module/tools.rs:816`。
- `crates/agentdash-application/src/runtime_gateway/session_actions.rs` — `mcp.call_tool` provider 返回 `AgentToolResult` 并序列化为 runtime output；见 `crates/agentdash-application/src/runtime_gateway/session_actions.rs:72`、`crates/agentdash-application/src/runtime_gateway/session_actions.rs:215`、`crates/agentdash-application/src/runtime_gateway/session_actions.rs:220`。

SessionEvent / persistence:

- `crates/agentdash-spi/src/session_persistence.rs` — `PersistedSessionEvent` 包含 `session_update_type/turn_id/entry_index/tool_call_id/notification`；见 `crates/agentdash-spi/src/session_persistence.rs:531`、`crates/agentdash-spi/src/session_persistence.rs:536`、`crates/agentdash-spi/src/session_persistence.rs:542`、`crates/agentdash-spi/src/session_persistence.rs:543`。
- `crates/agentdash-infrastructure/src/persistence/session_core.rs` — Postgres projection 从 `ItemStarted/ItemCompleted` 的 ThreadItem id 提取 `tool_call_id`；见 `crates/agentdash-infrastructure/src/persistence/session_core.rs:671`、`crates/agentdash-infrastructure/src/persistence/session_core.rs:674`、`crates/agentdash-infrastructure/src/persistence/session_core.rs:767`、`crates/agentdash-infrastructure/src/persistence/session_core.rs:769`。
- `crates/agentdash-agent-types/src/protocol.rs` — `AgentDashThreadItem::tool_call_id()` 实际返回 item id；见 `crates/agentdash-agent-types/src/protocol.rs:106`、`crates/agentdash-agent-types/src/protocol.rs:109`、`crates/agentdash-agent-types/src/protocol.rs:116`。

### 3. 最佳 guard 插入点和备选插入点

最佳插入点：`crates/agentdash-agent/src/agent_loop/tool_call.rs`，在 `finalize_executed_tool_call` 之后、`emit_tool_call_outcome` 之前，对最终 `AgentToolResult` 做一次通用 guard。

推荐形态：

```text
execute_prepared_tool_call_inner
  -> finalize_executed_tool_call
  -> guard_final_tool_result(context/tool_call/tool_name/args/result)
  -> emit_tool_call_outcome(guarded_result)
```

理由：

- 这是所有普通 PiAgent `DynAgentTool` 的汇合点，包括 application runtime tools、workspace module tools、MCP direct/relay 工具、错误结果、审批拒绝结果。
- guard 后的结果会同时进入 `ToolExecutionEnd` 和 `AgentMessage::ToolResult`，保证 BackboneEvent、SessionEvent、前端 tool card 和下一轮 provider request 看到同一个 bounded preview。
- 位于 `after_tool_call` / runtime delegate 之后，可以防止 hook 或 delegate 把大内容重新塞回 `result.content`。
- 位于 `emit_tool_call_outcome` 之前，可以避免大 payload 先进入 `AgentEvent::ToolExecutionEnd` 再被 stream mapper / persistence 截断。

需要设计为 agent-loop 层可调用的纯裁切/注解函数，但把 cache 写入 / lifecycle ref 生成做成注入式服务或 delegate effect，而不是让 `agentdash-agent-types` 依赖 application。当前 `AgentToolResult.details` 是唯一可承载 truncation/cache/lifecycle metadata 的现有字段；见 `crates/agentdash-agent-types/src/runtime/tool.rs:29`、`crates/agentdash-agent-types/src/runtime/tool.rs:30`。

备选插入点 A：`AgentTool` 包装器，在 tool assembly 产出的 `DynAgentTool` 外包一层 guarded tool。

- 位置：`crates/agentdash-application/src/session/tool_assembly.rs` 在收集 `all_tools` 后 wrap；见 `crates/agentdash-application/src/session/tool_assembly.rs:23`、`crates/agentdash-application/src/session/tool_assembly.rs:30`、`crates/agentdash-application/src/session/tool_assembly.rs:73`。
- 优点：application 层更容易拿到 session/project/backend/cache/lifecycle 服务，适合实现短期 cache 和 lifecycle ref。
- 缺点：覆盖不了 `prepare_tool_call` 中工具不存在、参数 schema 错误、before hook deny、审批拒绝、panic/error helper 等 agent-loop 内部生成的 `AgentToolResult`。这些通常不大，但“通用 guard”语义不完整。

备选插入点 B：`emit_tool_call_outcome` 内部统一 guard。

- 位置：`crates/agentdash-agent/src/agent_loop/tool_call.rs:615` 入口处先生成 `guarded_result`，然后 `ToolExecutionEnd` 和 `emit_tool_result_message` 都用 guarded result。
- 优点：最小改动，覆盖 `Immediate`、rejected、executed 三类结果，因为它们最终都调用 `emit_tool_call_outcome` 或 `emit_tool_result_message`。
- 缺点：当前审批 rejected 分支直接调用 `emit_tool_result_message`，未走 `ToolExecutionEnd`；见 `crates/agentdash-agent/src/agent_loop/tool_call.rs:116`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:204`。如果只改 `emit_tool_call_outcome` 会漏掉 rejected 分支。要么把 guard 放到 `emit_tool_result_message`，要么统一所有分支走同一个 guarded emit helper。

备选插入点 C：`stream_mapper.rs` 的 `ToolExecutionEnd` 映射阶段。

- 优点：可以保护 Backbone / SessionEvent 的 `ItemCompleted` payload；位置见 `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1025`。
- 缺点：太晚。`AgentMessage::ToolResult` 已经进入 `context.messages`，下一轮模型请求仍会拿到原文；`MessageStart/End(ToolResult)` event 也已经发出。只能作为 UI/事件防线，不是模型上下文防线。

备选插入点 D：provider bridge 转换 `AgentMessage::ToolResult` 为 provider request 时截断。

- 位置：`openai_responses_common.rs:168` 或各 bridge 的 `ToolResult` 分支。
- 优点：能保护 provider request。
- 缺点：BackboneEvent、SessionEvent、AgentState、context projection 仍可能持有大结果。还需要每个 provider bridge 分别实现，容易漂移。

不推荐作为主插入点：runtime gateway adapter / workspace module tools 局部改造。

- `RuntimeActionToolAdapter`、`WorkspaceModuleInvokeTool`、`McpCallToolProvider` 都只是结果来源之一；见 `crates/agentdash-application/src/runtime_gateway/tool_adapter.rs:115`、`crates/agentdash-application/src/workspace_module/tools.rs:547`、`crates/agentdash-application/src/runtime_gateway/session_actions.rs:215`。
- 局部加 guard 会留下其它 `AgentTool` 和 agent-loop 内部结果路径。

### 4. 需要保持的行为不变量

- Canonical bounded result invariant：guard 后的 `AgentToolResult` 必须是唯一进入 `ToolExecutionEnd`、`AgentMessage::ToolResult`、Backbone `ItemCompleted`、provider request 的版本。不能让模型看到 preview，而 SessionEvent 仍保存原文，或反过来。
- `is_error` 不变量：裁切不能改变工具执行成功/失败语义。`finalize_executed_tool_call` 最后设置 `result.is_error = is_error`；见 `crates/agentdash-agent/src/agent_loop/tool_call.rs:577`。guard 只能保留该值并补 metadata。
- `tool_call_id` / `call_id` 不变量：`AgentMessage::ToolResult` 必须保留原 `tool_call_id` 和 provider `call_id`，否则 Responses `function_call_output.call_id` 会断链；见 `crates/agentdash-agent-types/src/model/message.rs:194`、`crates/agentdash-agent-types/src/model/message.rs:203`、`crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_responses_common.rs:174`。
- Event ordering invariant：`ToolExecutionStart` / streaming tool-call started / `ToolExecutionEnd` 仍要映射到稳定 item lifecycle。Pi spec 要求 ToolCall start 为 `ItemStarted`，result 完成为 `ItemCompleted`；见 `.trellis/spec/backend/session/pi-agent-streaming.md` 的 ToolCall 映射章节。
- `entry_index` invariant：stream mapper 用 `ToolCallEmitState.entry_index` 关联 tool item；assistant MessageEnd 后才递增 entry_index；见 `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:40`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:73`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:755`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:757`。
- 多模态 invariant：`ContentPart::Image` 当前会进入 provider request 和 DynamicToolCall content_items；guard 不能把图片误拼成不可解析文本，也不能把 invalid image 数据重新放大；见 `crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_content.rs:60`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1138`。
- Provider schema invariant：工具 declaration 不应调用 runtime gateway 或产生副作用。guard/cache 写入不能放在 declaration assembly 阶段；相关规范见 `.trellis/spec/backend/runtime-gateway.md` 的 Runtime Tool Declaration Boundary。
- Tool schema / model-visible docs invariant：tool assembly 的 `RuntimeToolSchemaEntry` 与实际工具同源，guard 不应改变工具参数 schema；见 `.trellis/spec/backend/capability/tool-capability-pipeline.md` 的“工具 schema 与模型可见说明”。
- SessionEvent size invariant：Postgres `notification_json` 持久化的是整条 envelope；guard 必须在 envelope 生成前完成，才能真正防止 session_events 被大 payload 污染；见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:359`。
- Lifecycle ref invariant：discussion draft 明确 `SessionEvent = 模型实际看到的 bounded 内容`，`lifecycle ref = 大内容访问路径`，cache miss 是合法状态；见 `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/discussion-draft.md`。

### 5. 高风险测试点

必须覆盖的测试点：

- Agent loop 单元测试：构造返回超大 text 的 `AgentTool`，断言 `new_messages` 中的 `AgentMessage::ToolResult.content` 是 bounded preview，`details` 含 truncation/cache/lifecycle metadata，且 `is_error` 保持原值。
- Agent event 测试：同一大结果对应的 `AgentEvent::ToolExecutionEnd.result` 与 `MessageEnd(ToolResult)` 都是 bounded 版本，不能出现原文。
- Stream mapper 测试：大结果进入 `ToolExecutionEnd` 后，`BackboneEvent::ItemCompleted` 的 DynamicToolCall/ShellExec output 只含 preview 和 metadata，不含原始大文本。
- Provider bridge 测试：Responses `function_call_output.output` 只含 preview/ref 文本，不含原始大文本。特别要测 `ContentPart::Text` 拼接路径；见 `crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_content.rs:65`。
- shell_exec 测试：shell `aggregated_output` 当前取 `result.content[0].text`；裁切后要保留 exit code / status 可判定，避免 `exit_code` 被裁掉导致 UI 状态错误；见 `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1048`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1057`。
- `ToolExecutionUpdate` 测试：当前已有 `tool_execution_updates_preserve_full_tool_result_payload`，它验证 update/end 保留完整 payload；见 `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs:1215`。引入 guard 后需要决定 update 是否也受 bounded policy 约束，并更新该测试的意图。
- 审批拒绝 / 参数错误路径测试：`prepare_tool_call` 内部生成的 error result 和 approval rejected result 不走真实 `AgentTool::execute`。如果 guard 在 tool wrapper 层，这些路径不会被覆盖；见 `crates/agentdash-agent/src/agent_loop/tool_call.rs:307`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:701`。
- Runtime gateway / workspace module 测试：当 runtime provider output 本身已经是 `AgentToolResult` 时，guard 要在 provider details 合并之后仍保留 `runtime_action/runtime_trace/provider_details`；见 `crates/agentdash-application/src/runtime_gateway/tool_adapter.rs:116`、`crates/agentdash-application/src/runtime_gateway/tool_adapter.rs:119`。
- MCP relay/direct 测试：`mcp.call_tool` 返回 `AgentToolResult` 并序列化为 runtime output；要保证 MCP 大返回不会绕过 PiAgent guard；见 `crates/agentdash-application/src/runtime_gateway/session_actions.rs:215`、`crates/agentdash-application/src/runtime_gateway/session_actions.rs:220`。
- Persistence 集成测试：Postgres `session_events.notification_json` 不含原始大文本，`tool_call_id` 仍能从 completed item 提取。真实 Postgres 会提取 `ItemCompleted` item id；见 `crates/agentdash-infrastructure/src/persistence/session_core.rs:767`、`crates/agentdash-infrastructure/src/persistence/session_core.rs:770`。
- Context compaction / projection 测试：compaction summary request 应使用 bounded ToolResult，而不是通过 message history 重新读取原文。compaction cut point 依赖 tool_call/tool_result 成对边界，不能因 metadata/ref 改造破坏消息角色；见 `crates/agentdash-agent/src/compaction/mod.rs:155`、`crates/agentdash-agent/src/compaction/mod.rs:175`。
- Cache miss / expired 测试：lifecycle ref 读取失效时返回明确 bounded error result，不应自动恢复全量、不应写回 session_events 原文。

## Files Found

- `crates/agentdash-agent/src/agent_loop/tool_call.rs` — tool execution/finalize/emit 的汇合点，最适合插入 result guard。
- `crates/agentdash-agent/src/agent_loop.rs` — 把 tool result message 写回 `context.messages`，决定后续模型上下文。
- `crates/agentdash-agent/src/agent_loop/streaming.rs` — 每次 provider request 前从 context 构造 `BridgeRequest`，并处理 compaction。
- `crates/agentdash-agent/src/agent.rs` — Agent state/event sink 同步，会把 `MessageEnd(ToolResult)` 写入 AgentState。
- `crates/agentdash-agent-types/src/runtime/tool.rs` — `AgentToolResult` 和 `AgentTool` trait 合同。
- `crates/agentdash-agent-types/src/model/message.rs` — `AgentMessage::ToolResult` 模型上下文合同。
- `crates/agentdash-agent-types/src/model/content.rs` — `ContentPart` text/image/reasoning 基础结构。
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs` — PiAgent connector 把 AgentEvent 映射成 Backbone stream。
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` — ToolExecution* 到 Backbone item lifecycle 的转换。
- `crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_responses_common.rs` — ToolResult 到 Responses function_call_output 的 provider 转换。
- `crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_content.rs` — ToolResult content text/image 输出转换 helper。
- `crates/agentdash-application/src/session/tool_assembly.rs` — runtime tools 与 MCP tools 的统一装配边界。
- `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs` — workspace module runtime tools 的注入策略。
- `crates/agentdash-application/src/workspace_module/tools.rs` — workspace module tools 直接构造 `AgentToolResult` 的多个来源。
- `crates/agentdash-application/src/runtime_gateway/tool_adapter.rs` — RuntimeGateway output 到 `AgentToolResult` 的 adapter。
- `crates/agentdash-application/src/runtime_gateway/session_actions.rs` — `mcp.call_tool` runtime action 返回 `AgentToolResult`。
- `crates/agentdash-application/src/session/turn_processor.rs` — connector BackboneEnvelope 进入 persist/broadcast 的 per-turn processor。
- `crates/agentdash-application/src/session/eventing.rs` — BackboneEnvelope 持久化与 projection head 更新。
- `crates/agentdash-spi/src/session_persistence.rs` — `PersistedSessionEvent` 持久化 DTO。
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` — Postgres `append_event` 持久化整条 envelope。
- `crates/agentdash-infrastructure/src/persistence/session_core.rs` — session event 投影字段和 tool_call_id 提取逻辑。

## Code Patterns

- “执行后可改写结果”模式已经存在：`finalize_executed_tool_call` 允许 delegate / hook 覆盖 `content/details/is_error`，适合扩展为最后统一 guard；见 `crates/agentdash-agent/src/agent_loop/tool_call.rs:522`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:555`。
- “同一结果双写”模式：`emit_tool_call_outcome` 先发 ToolExecutionEnd，再构造 ToolResult message；见 `crates/agentdash-agent/src/agent_loop/tool_call.rs:621`、`crates/agentdash-agent/src/agent_loop/tool_call.rs:638`。
- “ThreadItem from AgentToolResult”模式：stream mapper decode `AgentToolResult` 并把 content 复制到 Codex content items；见 `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1127`。
- “runtime output 如果像 AgentToolResult 就透传”模式：RuntimeGateway adapter / workspace invoke 都会尝试 `serde_json::from_value::<AgentToolResult>`；见 `crates/agentdash-application/src/runtime_gateway/tool_adapter.rs:116`、`crates/agentdash-application/src/workspace_module/tools.rs:554`。
- “持久化索引从 Backbone 投影”模式：Postgres 先 `projection_from_envelope`，再写 `tool_call_id` 与 `notification_json`；见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:347`、`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:356`。

## External References

- 未使用外部引用。本研究只基于仓库代码、Trellis 规范和当前 task draft。

## Related Specs

- `.trellis/spec/backend/session/pi-agent-streaming.md` — Pi Agent `AgentEvent` 到 Backbone 的 ToolCall / Turn / token usage 映射契约。
- `.trellis/spec/cross-layer/backbone-protocol.md` — BackboneEnvelope / BackboneEvent / ThreadItem / Persisted Session Event 合同。
- `.trellis/spec/backend/runtime-gateway.md` — RuntimeGateway action 调用边界和 Runtime Tool Declaration Boundary。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` — runtime tool assembly、schema 同源、tool policy 和 workspace module surface 合同。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` — Rust contract -> TS generation、NDJSON stream 和 workspace module presentation contract。
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/discussion-draft.md` — 本任务关于 bounded preview、短期 cache、lifecycle ref、terminal 特殊处理的讨论背景。

## Caveats / Not Found

- 当前 `AgentToolResult` 没有一等 `preview/cache_ref/lifecycle_ref/truncation` 字段，只有 `details: Option<Value>` 可承载 metadata。若要变成稳定 wire contract，需要后续设计是否扩展类型或定义 details schema。
- 当前没有发现已实现的通用 result guard。相关讨论只存在于 task draft。
- 当前内存 persistence test adapter 固定 `tool_call_id: None`；真实 Postgres adapter 会从 item id 提取。实现测试时不要只依赖 memory adapter 判断 tool_call_id 行为。
- Pi stream mapper 的 persisted `tool_call_id` 实际来自 synthetic item id（`{turn_id}:{entry_index}:{tool_call_id}`），不是直接的原始 tool call id；这是现有行为，设计 lifecycle/cache ref 时需要显式决定使用原始 id、synthetic item id，还是两者都记录。
- `ToolExecutionUpdate` 当前可能携带 partial `AgentToolResult`，shell 输出会映射为 `CommandOutputDelta`。本研究主结论覆盖 final result；terminal/streaming update 是否需要同一 guard 要作为实现子任务单独设计。
- 本研究未修改代码、未运行测试。
