# Pi Agent Core 对齐追踪文档

> **目标**：将 `agentdash-agent` 的 Agent Loop 核心语义严格对齐 `pi-agent-core`（`@mariozechner/pi-agent-core`），
> 在保持 Rust 惯用设计的前提下确保行为等价。本文档作为长期同步维护的唯一真相源。

---

## 参考版本

| 项目 | 版本 / Commit | 本地路径 |
|------|--------------|----------|
| pi-agent-core | `0.58.4` | `references/pi-mono/packages/agent/` |
| agentdash-agent | `HEAD` | `crates/agentdash-agent/` |

> **更新约定**：当 pi-agent-core 发布新版本时，拉取最新代码后对照本文档逐项检查 `[差异]` 条目并更新。

---

## 一、类型层对齐 (`types.ts` ↔ `types.rs`)

### 1.1 AgentMessage

| Pi 字段 / 能力 | Pi 位置 | AgentDash 状态 | 说明 |
|----------------|---------|---------------|------|
| `Message` union (user/assistant/toolResult) | `types.ts:245` | ✅ 已对齐 | Rust 用 tagged enum 实现 |
| `CustomAgentMessages` 扩展机制 | `types.ts:236-238` | ⏭️ 暂不实现 | TypeScript 通过 declaration merging 扩展；Rust 中可通过 trait + enum dispatch 实现，但当前无扩展需求 |
| `timestamp` 字段 | assistant/user/toolResult 均有 | ✅ 已对齐 | P2 轮实现 — 所有变体新增 `Option<u64>` timestamp，构造方法自动设置 `now_millis()` |
| `stopReason` (assistant) | `"stop" \| "length" \| "toolUse" \| "error" \| "aborted"` | ✅ 已对齐 | P1 轮实现 `StopReason` enum + `stop_reason` 字段 |
| `errorMessage` (assistant) | Pi assistant 消息可携带错误文本 | ✅ 已对齐 | P1 轮实现 `error_message` 字段 + `error_assistant()` 构造方法 |
| `usage` (assistant) | `{ input, output, cacheRead, cacheWrite, totalTokens, cost }` | ✅ 已对齐 | P1 轮实现 `TokenUsage { input, output }` — 基础字段；cacheRead 等高级字段按需追加 |
| `api` / `provider` / `model` (assistant) | 模型元数据 | ⏭️ 后续 | 用于日志和 UI 展示，需 Bridge 层暴露 |
| `toolName` (toolResult) | Pi 的 `ToolResultMessage` 有 `toolName` | ✅ 已对齐 | P2 轮实现 — `ToolResult` 新增 `tool_name: Option<String>`，`emit_tool_call_outcome` 自动填充 |
| `details` (toolResult) | 工具结果详情（泛型） | ✅ 已对齐 | P2 轮实现 — `ToolResult` 新增 `details: Option<serde_json::Value>`，从 `AgentToolResult.details` 透传 |

### 1.2 AgentEvent

| Pi 事件 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| `agent_start` | `types.ts:297` | ✅ 已对齐 | |
| `agent_end { messages }` | `types.ts:298` | ✅ 已对齐 | |
| `turn_start` | `types.ts:300` | ✅ 已对齐 | |
| `turn_end { message, toolResults }` | `types.ts:301` | ✅ 已对齐 | P0 轮已实现 |
| `message_start { message }` | `types.ts:303` | ✅ 已对齐 | P0 轮已实现 |
| `message_update { message, assistantMessageEvent }` | `types.ts:305` | ⚠️ 部分对齐 | 用 `MessageDelta { text }` + `ToolExecutionUpdate` 覆盖核心场景；细粒度子事件 (thinking_*) 待思考模型对接 |
| `message_end { message }` | `types.ts:306` | ✅ 已对齐 | |
| `tool_execution_start { toolCallId, toolName, args }` | `types.ts:308` | ✅ 已对齐 | |
| `tool_execution_update { toolCallId, toolName, args, partialResult }` | `types.ts:309` | ✅ 已对齐 | P2 轮通过 `on_update` 回调穿透实现 |
| `tool_execution_end { toolCallId, toolName, result, isError }` | `types.ts:310` | ✅ 已对齐 | |

### 1.3 AgentTool

| Pi 能力 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| `name` / `description` / `parameters` | `Tool` base | ✅ 已对齐 | AgentDash 用 `parameters_schema()` 对应 Pi 的 TypeBox `parameters` |
| `label` (人类可读标签) | `types.ts:275` | ✅ 已对齐 | P2 轮实现 — `AgentTool::label()` 默认返回 `name()` |
| `execute(toolCallId, params, signal?, onUpdate?)` | `types.ts:276-281` | ✅ 已对齐 | P2 轮实现 — 签名更新为 `execute(id, args, cancel, on_update)` |
| `AgentToolResult<T> { content, details }` | `types.ts:262-267` | ✅ 已对齐 | P2 轮实现 — `AgentToolResult` 新增 `details: Option<serde_json::Value>` |
| `AgentToolUpdateCallback` | `types.ts:270` | ✅ 已对齐 | P2 轮实现 — `ToolUpdateCallback = Arc<dyn Fn(AgentToolResult) + Send + Sync>` |

### 1.4 AgentContext

| Pi 字段 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| `systemPrompt` | `types.ts:286` | ✅ 已对齐 | |
| `messages` | `types.ts:287` | ✅ 已对齐 | |
| `tools?` (optional) | `types.ts:288` | ⚠️ 差异 | AgentDash 为 `Vec<DynAgentTool>`（必填） |

### 1.5 AgentLoopConfig

| Pi 字段 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| `model` | `types.ts:97` | ⏭️ 不适用 | Rust 侧模型在 `LlmBridge` 内部管理 |
| `convertToLlm` | `types.ts:125` | ✅ 已对齐 | P0 轮实现为 `Option<ConvertToLlmFn>` |
| `transformContext` | `types.ts:147` | ✅ 已对齐 | P0 轮实现为 `Option<TransformContextFn>` |
| `getApiKey` | `types.ts:157` | ⏭️ 不适用 | Rust 侧 API key 在 `LlmBridge` 实现中管理 |
| `getSteeringMessages` | `types.ts:170` | ✅ 已对齐 | |
| `getFollowUpMessages` | `types.ts:183` | ✅ 已对齐 | |
| `toolExecution` | `types.ts:192` | ✅ 已对齐 | P0 轮实现，默认 Parallel |
| `beforeToolCall` | `types.ts:200` | ✅ 已对齐 | P0 轮实现为 `Option<BeforeToolCallFn>` |
| `afterToolCall` | `types.ts:213` | ✅ 已对齐 | P0 轮实现为 `Option<AfterToolCallFn>` |
| `temperature` / `max_tokens` | 继承自 `SimpleStreamOptions` | ✅ 已对齐 | |
| `reasoning` / `thinkingBudgets` | 继承自 `SimpleStreamOptions` | ⚠️ 部分对齐 | P2 轮新增 `ThinkingLevel` enum 和 `AgentConfig.thinking_level`；实际传递到 Bridge 层待对接 |
| `sessionId` / `transport` / `maxRetryDelayMs` | 继承自 `SimpleStreamOptions` | ⏭️ 不适用 | Rust 侧在 Bridge 层管理 |

### 1.6 其他类型

| Pi 类型 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| `ToolExecutionMode` | `types.ts:35` | ✅ 已对齐 | P0 轮新增 |
| `BeforeToolCallResult` | `types.ts:46-49` | ✅ 已对齐 | P0 轮新增 |
| `AfterToolCallResult` | `types.ts:62-66` | ✅ 已对齐 | P2 轮补齐 `details` 字段 |
| `BeforeToolCallContext` | `types.ts:69-78` | ✅ 已对齐 | P0 轮新增（使用 `&'a` 引用替代 clone） |
| `AfterToolCallContext` | `types.ts:81-94` | ✅ 已对齐 | P0 轮新增 |
| `ThinkingLevel` | `types.ts:220` | ✅ 已对齐 | P2 轮新增 — `Off / Minimal / Low / Medium / High / Xhigh` |
| `AgentState` | `types.ts:250-260` | ⚠️ 部分对齐 | P2 轮 Agent 增加 `is_running`、`subscribe()` 等状态管理；完整 AgentState struct 抽象待后续 |
| `StreamFn` | `types.ts:24-26` | ⏭️ 不适用 | Rust 用 `LlmBridge` trait 替代 |

---

## 二、Agent Loop 流程对齐 (`agent-loop.ts` ↔ `agent_loop.rs`)

### 2.1 入口函数

| Pi 函数 | Pi 位置 | AgentDash 对应 | 状态 | 说明 |
|---------|---------|---------------|------|------|
| `agentLoop(prompts, context, config, signal?, streamFn?)` | `agent-loop.ts:31-54` | `agent_loop(prompts, context, config, bridge, events, cancel)` | ✅ 已对齐 | Pi 返回 `EventStream`，AgentDash 通过 channel 推送 |
| `agentLoopContinue(context, config, signal?, streamFn?)` | `agent-loop.ts:64-93` | `agent_loop_continue(...)` | ✅ 已对齐 | P0 轮增加安全检查 |
| `runAgentLoop(...)` | `agent-loop.ts:95-118` | 逻辑内联在 `agent_loop()` | ✅ 已对齐 | |
| `runAgentLoopContinue(...)` | `agent-loop.ts:120-143` | 逻辑内联在 `agent_loop_continue()` | ✅ 已对齐 | |

### 2.2 runLoop 主循环

> **这是最核心的对齐点。** Pi 的 `runLoop` 使用内外双层循环，AgentDash 已对齐实现。

| Pi 行为 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| **循环开始时轮询 steering** | `agent-loop.ts:165` | ✅ 已对齐 | P0 轮实现 |
| **外循环**：处理 follow-up 消息 | `agent-loop.ts:168` `while(true)` | ✅ 已对齐 | `'outer` loop |
| **内循环**：处理 tool calls + steering | `agent-loop.ts:172` | ✅ 已对齐 | `'inner` loop |
| **pending messages 注入** | `agent-loop.ts:180-188` | ✅ 已对齐 | 发出 message_start/end 后注入 context |
| **`streamAssistantResponse()`** | `agent-loop.ts:238-331` | ✅ 已对齐 | `stream_assistant_response()` |
| **`transformContext` 调用** | `agent-loop.ts:247-249` | ✅ 已对齐 | P0 轮实现 |
| **`convertToLlm` 调用** | `agent-loop.ts:252` | ✅ 已对齐 | P0 轮实现 |
| **`stopReason` 检查** | `agent-loop.ts:194-198` | ✅ 已对齐 | P1 轮实现 — error/aborted 提前退出 |
| **prompt 消息事件** | `agent-loop.ts:111-114` | ✅ 已对齐 | P0 轮实现 |
| **turn 内 steering 轮询** | `agent-loop.ts:216` | ✅ 已对齐 | AgentDash 在每个 turn 的工具执行后轮询 |

### 2.3 工具执行

| Pi 行为 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| **`executeToolCalls` 入口分发** | `agent-loop.ts:336-348` | ✅ 已对齐 | P0 轮实现 |
| **`prepareToolCall`** | `agent-loop.ts:458-507` | ✅ 已对齐 | P0 轮实现三阶段工具执行 |
| **参数校验 `validateToolArguments`** | `agent-loop.ts:475` | ⚠️ 部分对齐 | AgentDash 将校验委托给工具自身的 `serde_json::from_value`（schema 校验未独立实现） |
| **`beforeToolCall` 钩子** | `agent-loop.ts:476-493` | ✅ 已对齐 | P0 轮实现 |
| **`executePreparedToolCall`** | `agent-loop.ts:509-544` | ✅ 已对齐 | P2 轮补齐 `on_update` 回调传递 |
| **`finalizeExecutedToolCall`** | `agent-loop.ts:546-580` | ✅ 已对齐 | P2 轮补齐 `details` 透传 |
| **并行执行 `executeToolCallsParallel`** | `agent-loop.ts:390-438` | ✅ 已对齐 | P0 轮实现，P2 轮补齐每个工具独立 `on_update` |
| **`tool_execution_update` 事件** | 在 `executePreparedToolCall` 中通过 `onUpdate` 触发 | ✅ 已对齐 | P2 轮实现 `build_on_update()` 闭包构建 |
| **工具结果消息事件** | `agent-loop.ts:613-614` | ✅ 已对齐 | P0 轮实现 — 每个 tool result 发出 `message_start` / `message_end` |

### 2.4 流式响应处理 (`streamAssistantResponse`)

| Pi 行为 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| **`transformContext` → `convertToLlm` 管线** | `agent-loop.ts:246-252` | ✅ 已对齐 | P0 轮实现 |
| **`message_start` 携带 partial message** | `agent-loop.ts:282` | ✅ 已对齐 | P0 轮实现 `MessageStart { message }` |
| **`message_update` 细粒度事件** | `agent-loop.ts:294-302` | ⚠️ 部分对齐 | `MessageDelta { text }` 覆盖文本场景；thinking_* 子事件待思考模型对接 |
| **流式 partial message 维护** | `agent-loop.ts:273-280` | ⚠️ 部分对齐 | AgentDash 在流结束后构建完整消息 |
| **`message_end` 前处理 done/error** | `agent-loop.ts:306-318` | ✅ 已对齐 | |

---

## 三、Agent 高层封装对齐 (`agent.ts` ↔ `agent.rs`)

### 3.1 构造与配置

| Pi 能力 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| `AgentOptions` 完整配置 | `agent.ts:41-114` | ✅ 已对齐 | `AgentConfig` 含全部核心字段 |
| `convertToLlm` 可配置 | `agent.ts:48` | ✅ 已对齐 | P0 轮 `Option<ConvertToLlmFn>` |
| `transformContext` 可配置 | `agent.ts:54` | ✅ 已对齐 | P0 轮 `Option<TransformContextFn>` |
| `steeringMode` / `followUpMode` | `agent.ts:59-64` | ✅ 已对齐 | P0 轮 `QueueMode` enum |
| `streamFn` 可注入 | `agent.ts:69` | ⏭️ 不适用 | Rust 用 `LlmBridge` trait |
| `toolExecution` 模式 | `agent.ts:107` | ✅ 已对齐 | P0 轮 |
| `beforeToolCall` / `afterToolCall` | `agent.ts:110-113` | ✅ 已对齐 | P0 轮 |
| `thinkingLevel` | `agent.ts:119` | ✅ 已对齐 | P2 轮 `ThinkingLevel` enum + `set_thinking_level()` |

### 3.2 状态管理

| Pi 能力 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| `AgentState` 统一状态对象 | `agent.ts:117-127` | ⚠️ 部分对齐 | AgentDash 状态分散在 Agent 各字段中；未封装为独立 struct |
| `isStreaming` | `agent.ts:124` | ✅ 已对齐 | `Agent.is_running` |
| `streamMessage` | `agent.ts:125` | ⏭️ 后续 | 流式 partial message 缓存 |
| `pendingToolCalls: Set<string>` | `agent.ts:126` | ⏭️ 后续 | |
| `error` | `agent.ts:127` | ✅ 已对齐 | 通过 `error_assistant()` + `is_error_or_aborted()` |

### 3.3 事件分发

| Pi 能力 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| `subscribe(fn) → unsubscribe` | `agent.ts:260-263` | ✅ 已对齐 | P2 轮实现 — `Agent.subscribe()` 返回 `EventReceiver`，基于 `broadcast::channel` 支持多订阅者 |
| `_processLoopEvent` 状态同步 | `agent.ts:458-500` | ⏭️ 后续 | Pi 在收到事件时同步更新 `AgentState`；AgentDash 当前无独立 AgentState struct |

### 3.4 Steering / Follow-up 队列

| Pi 能力 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| `steer(m)` / `followUp(m)` | `agent.ts:310-320` | ✅ 已对齐 | |
| `dequeueSteeringMessages` (按模式出队) | `agent.ts:339-352` | ✅ 已对齐 | P0 轮实现 `dequeue_messages` + `QueueMode` |
| `dequeueFollowUpMessages` (按模式出队) | `agent.ts:354-367` | ✅ 已对齐 | 同上 |
| `clearSteeringQueue` / `clearFollowUpQueue` / `clearAllQueues` | `agent.ts:322-333` | ✅ 已对齐 | P0 轮实现 |
| `hasQueuedMessages()` | `agent.ts:335-337` | ✅ 已对齐 | P0 轮实现 |

### 3.5 生命周期

| Pi 能力 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| `prompt(message)` | `agent.ts:392-425` | ✅ 已对齐 | |
| `continue()` 安全检查 | `agent.ts:430-456` | ✅ 已对齐 | P0 轮实现 |
| `abort()` | `agent.ts:373-375` | ✅ 已对齐 | |
| `waitForIdle()` | `agent.ts:377-379` | ✅ 已对齐 | P1 轮实现 |
| `reset()` | `agent.ts:381-389` | ✅ 已对齐 | P1 轮增强 |
| 错误封装为 AssistantMessage | `agent.ts:573-595` | ✅ 已对齐 | P1 轮实现 |

---

## 四、Proxy 模块 (`proxy.ts`)

| Pi 能力 | Pi 位置 | AgentDash 状态 | 说明 |
|---------|---------|---------------|------|
| `streamProxy(model, context, options)` | `proxy.ts:85-206` | ⏭️ 暂不实现 | HTTP 代理调用 LLM，用于浏览器环境；AgentDash 为桌面应用，当前不需要 |
| `ProxyAssistantMessageEvent` | `proxy.ts:36-57` | ⏭️ 暂不实现 | |
| `processProxyEvent` | `proxy.ts:211-340` | ⏭️ 暂不实现 | |

---

## 五、实施优先级

### P0 — 循环语义等价 ✅

1. ✅ **`types.rs`**：增加 `ToolExecutionMode`、`BeforeToolCallResult`、`AfterToolCallResult`、`BeforeToolCallContext`、`AfterToolCallContext`
2. ✅ **`types.rs`**：`AgentEvent::TurnEnd` 携带 `message` + `tool_results`；`MessageStart` 携带 `message`
3. ✅ **`types.rs`**：新增 `AgentEvent::MessageUpdate` 和 `ToolExecutionUpdate`
4. ✅ **`types.rs`**：`AgentTool::execute` 增加 `CancellationToken` 参数
5. ✅ **`agent_loop.rs`**：`AgentLoopConfig` 增加 `convert_to_llm`、`transform_context`、`tool_execution`、`before_tool_call`、`after_tool_call`
6. ✅ **`agent_loop.rs`**：重构 `run_loop` 为 Pi 的内外双循环结构
7. ✅ **`agent_loop.rs`**：循环开始时轮询 steering
8. ✅ **`agent_loop.rs`**：`agentLoopContinue` 增加安全检查
9. ✅ **`agent_loop.rs`**：实现 `prepare_tool_call` / `execute_prepared_tool_call` / `finalize_executed_tool_call` 三阶段工具执行
10. ✅ **`agent_loop.rs`**：实现并行工具执行 (`execute_tool_calls_parallel`)
11. ✅ **`agent_loop.rs`**：为 prompt 消息和 tool result 消息发出 `message_start` / `message_end` 事件
12. ✅ **`agent.rs`**：`AgentConfig` 增加 `steering_mode`、`follow_up_mode`、`tool_execution`、`before_tool_call`、`after_tool_call`、`convert_to_llm`、`transform_context`
13. ✅ **`agent.rs`**：实现 `dequeue_steering_messages` / `dequeue_follow_up_messages` 按模式出队
14. ✅ **`agent.rs`**：`continue_loop` 增加安全检查

### P1 — 状态追踪与 API 完善 ✅

15. ✅ **`types.rs`**：`AgentMessage::Assistant` 增加 `stop_reason`、`error_message`、`usage`；新增 `StopReason` enum、`TokenUsage` struct
16. ✅ **`agent.rs`**：增加 `is_running` + `idle_notify: Arc<Notify>` 运行状态追踪
17. ✅ **`agent.rs`**：增加 `wait_for_idle()`；增强 `reset()` — abort + await idle + 清空全部状态
18. ✅ **`types.rs`**：`error_assistant()` 构造方法 + `is_error_or_aborted()` 检查
19. ✅ **`agent_loop.rs`**：`run_loop` 中 `stream_assistant_response` 后检查 `stopReason`；`stream_assistant_response` 传播 `usage` 和 `stop_reason`

### P2 — 增强能力 ✅

20. ✅ **`types.rs`**：`AgentTool::execute` 增加 `on_update: Option<ToolUpdateCallback>` 参数
21. ✅ **`types.rs`**：`AgentToolResult` 增加 `details: Option<serde_json::Value>`
22. ✅ **`types.rs`**：`AgentTool` 增加 `label()` 默认方法
23. ✅ **`types.rs`**：所有 `AgentMessage` 变体增加 `timestamp: Option<u64>` 字段
24. ✅ **`event_stream.rs`**：从 `mpsc::unbounded` 迁移到 `broadcast::channel`，支持多订阅者
25. ✅ **`types.rs`**：新增 `ThinkingLevel` enum（`Off/Minimal/Low/Medium/High/Xhigh`）
26. ✅ **`agent.rs`**：增加 `subscribe()` 方法（多订阅者模型）、`set_thinking_level()`
27. ✅ **`types.rs`**：`ToolResult` 增加 `tool_name: Option<String>` 字段
28. ✅ **`types.rs`**：`AfterToolCallResult` 增加 `details` 字段
29. ✅ **`agent_loop.rs`**：`build_on_update()` 构建 on_update 回调；并行执行中每个工具获得独立回调
30. ✅ **所有工具实现**：`builtins.rs` (5 个) + `pi_agent_mcp.rs` (1 个) 更新 `execute` 签名

### ⏭️ 不适用 / 暂缓

- `model` 在 `AgentLoopConfig` 中（Rust 用 Bridge trait）
- `getApiKey`（Rust 在 Bridge 内管理）
- `streamFn` / `StreamFn`（Rust 用 `LlmBridge` trait）
- `sessionId` / `transport` / `maxRetryDelayMs`（Bridge 层负责）
- `proxy.ts` 全部（桌面应用不需要）
- `CustomAgentMessages` 扩展机制（当前无需求）
- `api` / `provider` / `model` 模型元数据（需 Bridge 层暴露）
- `AgentState` 统一状态对象抽象（当前分散在 Agent 字段中，功能等价）
- `_processLoopEvent` 状态同步（待 AgentState struct 抽象后实现）
- 流式 partial message 维护 / `streamMessage`（待深度流式场景需求）
- `pendingToolCalls: Set<string>` 追踪（待 UI 对接需求）

---

## 六、逐项变更记录

> 每完成一项，在此记录变更摘要和 commit hash。

| # | 变更 | 日期 | Commit |
|---|------|------|--------|
| 1 | `types.rs`: 新增 `ToolExecutionMode`、`BeforeToolCallResult/Context`、`AfterToolCallResult/Context` | 2026-03-17 | P0 batch |
| 2 | `types.rs`: `AgentEvent` 对齐 — `TurnEnd` 携带 message+tool_results、`MessageStart` 携带 message、新增 `ToolExecutionUpdate` | 2026-03-17 | P0 batch |
| 3 | `types.rs`: `AgentTool::execute` 增加 `CancellationToken` 参数 | 2026-03-17 | P0 batch |
| 4 | `types.rs`: `AgentError` 新增 `ContinueError` 变体 | 2026-03-17 | P0 batch |
| 5 | `agent_loop.rs`: 完全重写 — `AgentLoopConfig` 新增 `convert_to_llm`/`transform_context`/`tool_execution`/`before_tool_call`/`after_tool_call` | 2026-03-17 | P0 batch |
| 6 | `agent_loop.rs`: `run_loop` 重构为 Pi 内外双循环结构，循环开始前轮询 steering | 2026-03-17 | P0 batch |
| 7 | `agent_loop.rs`: 实现三阶段工具执行 (prepare → execute → finalize) 和并行执行 | 2026-03-17 | P0 batch |
| 8 | `agent_loop.rs`: `agent_loop_continue` 增加安全检查 | 2026-03-17 | P0 batch |
| 9 | `agent_loop.rs`: prompt 消息和 tool result 消息发出 `message_start`/`message_end` 事件 | 2026-03-17 | P0 batch |
| 10 | `agent.rs`: `AgentConfig` 扩展 — 新增 `convert_to_llm`/`transform_context`/`steering_mode`/`follow_up_mode`/`tool_execution`/`before_tool_call`/`after_tool_call` | 2026-03-17 | P0 batch |
| 11 | `agent.rs`: 新增 `QueueMode` enum、`dequeue_messages` 按模式出队 | 2026-03-17 | P0 batch |
| 12 | `agent.rs`: 新增 `clear_steering_queue`/`clear_follow_up_queue`/`has_queued_messages`/`reset` | 2026-03-17 | P0 batch |
| 13 | `bridge.rs`: `BridgeRequest` 新增 `llm_messages` 字段，支持预转换消息 | 2026-03-17 | P0 batch |
| 14 | 下游 `pi_agent_mcp.rs`: `AgentTool::execute` 签名更新 | 2026-03-17 | P0 batch |
| 15 | `types.rs`: 新增 `StopReason` enum、`TokenUsage` struct；`AgentMessage::Assistant` 增加 `stop_reason`/`error_message`/`usage` 字段 | 2026-03-17 | P1 batch |
| 16 | `types.rs`: 新增 `error_assistant()` 构造方法 + `is_error_or_aborted()` 检查方法 | 2026-03-17 | P1 batch |
| 17 | `agent.rs`: 增加 `is_running` + `idle_notify` + `wait_for_idle()`；增强 `reset()` | 2026-03-17 | P1 batch |
| 18 | `agent_loop.rs`: `stream_assistant_response` 传播 `usage`/`stop_reason`；`run_loop` 检查 `stopReason` 提前退出 | 2026-03-17 | P1 batch |
| 19 | `convert.rs`: pattern matching 适配 `Assistant` 新增字段 (`..`) | 2026-03-17 | P1 batch |
| 20 | `lib.rs`: 导出 `StopReason`、`TokenUsage` | 2026-03-17 | P1 batch |
| 21 | `types.rs`: 所有 `AgentMessage` 变体增加 `timestamp` 字段；`ToolResult` 增加 `tool_name`/`details` | 2026-03-17 | P2 batch |
| 22 | `types.rs`: `AgentToolResult` 新增 `details` 字段；`AfterToolCallResult` 新增 `details` 字段 | 2026-03-17 | P2 batch |
| 23 | `types.rs`: `AgentTool` trait 新增 `label()` 默认方法 | 2026-03-17 | P2 batch |
| 24 | `types.rs`: 新增 `ToolUpdateCallback` 类型；`execute` 签名增加 `on_update` 参数 | 2026-03-17 | P2 batch |
| 25 | `types.rs`: 新增 `ThinkingLevel` enum | 2026-03-17 | P2 batch |
| 26 | `agent_loop.rs`: `build_on_update()` 构建 on_update 回调；`emit_tool_call_outcome` 传递 tool_name/details | 2026-03-17 | P2 batch |
| 27 | `event_stream.rs`: 从 `mpsc::unbounded` 迁移到 `broadcast::channel` 多订阅者 | 2026-03-17 | P2 batch |
| 28 | `agent.rs`: 新增 `subscribe()` + `set_thinking_level()` + persistent event sender | 2026-03-17 | P2 batch |
| 29 | `builtins.rs`: 5 个工具 execute 签名更新 + AgentToolResult 补 details 字段 | 2026-03-17 | P2 batch |
| 30 | `pi_agent_mcp.rs`: McpToolAdapter execute 签名更新 + AgentToolResult 补 details 字段 | 2026-03-17 | P2 batch |
| 31 | `convert.rs`: User 匹配增加 `..`；`assistant_from_llm_content` 补 `timestamp` | 2026-03-17 | P2 batch |
| 32 | `lib.rs`: 导出 `ThinkingLevel`、`ToolUpdateCallback` | 2026-03-17 | P2 batch |

---

## 七、Pi Agent Core 版本变更日志摘要

> 当 pi-agent-core 发布新版时，在此记录与 AgentDash 对齐相关的变更。

| Pi 版本 | 变更摘要 | AgentDash 影响 | 状态 |
|---------|---------|---------------|------|
| `0.58.4` | 基准版本 | 本文档基于此版本编写 | ✅ P0/P1/P2 已对齐 |
