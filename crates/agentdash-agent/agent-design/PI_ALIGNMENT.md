# Pi Agent Core 对齐追踪文档

> **目标**：将 `agentdash-agent` 的 Agent Loop 核心语义严格对齐 `pi-agent-core`（`@mariozechner/pi-agent-core`），
> 在保持 Rust 惯用设计的前提下确保行为等价。本文档作为长期同步维护的唯一真相源。

---

## 参考版本

| 项目 | 版本 / Commit | 本地路径 |
|------|--------------|----------|
| pi-agent-core | `0.58.4` | `references/pi-mono/packages/agent/` |
| agentdash-agent | `HEAD` | `crates/agentdash-agent/` |

> **更新约定**：当 pi-agent-core 发布新版本时，拉取最新代码后对照本文档逐项检查差异并更新。

---

## 一、类型层对齐 (`types.ts` ↔ `types.rs`)

### 1.1 AgentMessage

| Pi 字段 / 能力 | AgentDash 状态 | 说明 |
|----------------|---------------|------|
| `Message` union (user/assistant/toolResult) | ✅ | tagged enum |
| `CustomAgentMessages` 扩展机制 | ⏭️ 暂不实现 | 无扩展需求 |
| `timestamp` | ✅ | 所有变体 `Option<u64>` |
| `stopReason` (assistant) | ✅ | `StopReason` enum |
| `errorMessage` (assistant) | ✅ | `error_message` 字段 |
| `usage` (assistant) | ✅ | `TokenUsage { input, output }` |
| `api` / `provider` / `model` (assistant) | ⏭️ | 需 Bridge 层暴露模型元数据 |
| `toolName` (toolResult) | ✅ | `tool_name: Option<String>` |
| `details` (toolResult) | ✅ | `details: Option<serde_json::Value>` |

### 1.2 AgentEvent

| Pi 事件 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `agent_start` / `agent_end` | ✅ | |
| `turn_start` / `turn_end { message, toolResults }` | ✅ | |
| `message_start { message }` | ✅ | |
| `message_update { message, assistantMessageEvent }` | ✅ | P3 轮实现 `MessageUpdate { message, event: AssistantStreamEvent }` |
| `message_end { message }` | ✅ | |
| `tool_execution_start/update/end` | ✅ | |

### 1.3 AgentTool

| Pi 能力 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `name` / `description` / `parameters` | ✅ | `parameters_schema()` |
| `label` | ✅ | 默认返回 `name()` |
| `execute(id, params, signal?, onUpdate?)` | ✅ | `execute(id, args, cancel, on_update)` |
| `AgentToolResult<T> { content, details }` | ✅ | `details: Option<serde_json::Value>` |
| `AgentToolUpdateCallback` | ✅ | `ToolUpdateCallback = Arc<dyn Fn(AgentToolResult) + Send + Sync>` |

### 1.4 AgentContext

| Pi 字段 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `systemPrompt` / `messages` | ✅ | |
| `tools?` (optional) | ⚠️ | AgentDash 为 `Vec<DynAgentTool>`（空 Vec ≈ None） |

### 1.5 AgentLoopConfig

| Pi 字段 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `convertToLlm` | ✅ | `Option<ConvertToLlmFn>` |
| `transformContext` | ✅ | `Option<TransformContextFn>` |
| `getSteeringMessages` / `getFollowUpMessages` | ✅ | |
| `toolExecution` | ✅ | `ToolExecutionMode` |
| `beforeToolCall` / `afterToolCall` | ✅ | |
| `temperature` / `max_tokens` | ✅ | |
| `reasoning` / `thinkingBudgets` | ⚠️ | `ThinkingLevel` enum 已定义，传递到 Bridge 待对接 |
| `model` / `getApiKey` / `streamFn` | ⏭️ | Bridge trait 内部管理 |
| `sessionId` / `transport` / `maxRetryDelayMs` | ⏭️ | Bridge 层负责 |

### 1.6 其他类型

| Pi 类型 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `ToolExecutionMode` | ✅ | |
| `BeforeToolCallResult` / `Context` | ✅ | |
| `AfterToolCallResult` / `Context` | ✅ | 含 `details` 字段 |
| `ThinkingLevel` | ✅ | `Off/Minimal/Low/Medium/High/Xhigh` |
| `AgentState` | ✅ | P3 轮实现完整 struct |
| `AssistantStreamEvent` | ✅ | P3 轮新增 — TextDelta / ToolCallDelta |
| `StreamFn` | ⏭️ | `LlmBridge` trait 替代 |

---

## 二、Agent Loop 流程对齐 (`agent-loop.ts` ↔ `agent_loop.rs`)

### 2.1 入口与主循环

| Pi 行为 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `agentLoop` / `agentLoopContinue` 入口 | ✅ | 含安全检查 |
| 内外双循环结构（steering + follow-up） | ✅ | `'outer` / `'inner` loop |
| `transformContext` → `convertToLlm` 管线 | ✅ | |
| `stopReason` 检查 | ✅ | error/aborted 提前退出 |
| prompt / tool result 消息事件 | ✅ | message_start / message_end |

### 2.2 流式响应

| Pi 行为 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `message_start` 携带 partial message | ✅ | P3 轮重构 — 首次 delta 时发出 |
| `message_update { message, event }` | ✅ | P3 轮实现 — 每次 delta 携带 partial message 快照 |
| 流式 partial message 维护 | ✅ | P3 轮实现 — 流中累加文本到 partial，同步到 context.messages |
| `message_end` 前处理 done/error | ✅ | |

### 2.3 工具执行

| Pi 行为 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| 三阶段执行 (prepare → execute → finalize) | ✅ | |
| 参数校验 `validateToolArguments` | ⚠️ | 委托给工具自身的 serde 反序列化 |
| `beforeToolCall` / `afterToolCall` 钩子 | ✅ | |
| 并行执行 + 独立 `on_update` 回调 | ✅ | |
| `tool_execution_update` 事件 | ✅ | |

---

## 三、Agent 高层封装对齐 (`agent.ts` ↔ `agent.rs`)

### 3.1 构造与配置

| Pi 能力 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `AgentOptions` 完整配置 | ✅ | `AgentConfig` |
| `convertToLlm` / `transformContext` | ✅ | |
| `steeringMode` / `followUpMode` | ✅ | `QueueMode` enum |
| `toolExecution` / `beforeToolCall` / `afterToolCall` | ✅ | |
| `thinkingLevel` | ✅ | `set_thinking_level()` |
| `streamFn` 可注入 | ⏭️ | `LlmBridge` trait |

### 3.2 状态管理

| Pi 能力 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `AgentState` 统一状态对象 | ✅ | P3 轮实现 — `Arc<Mutex<AgentState>>` |
| `isStreaming` | ✅ | `AgentState.is_streaming` |
| `streamMessage` | ✅ | P3 轮实现 — `AgentState.stream_message` |
| `pendingToolCalls: Set<string>` | ✅ | P3 轮实现 — `AgentState.pending_tool_calls: HashSet<String>` |
| `error` | ✅ | `AgentState.error` |
| `_processLoopEvent` 状态同步 | ✅ | P3 轮实现 — `process_event()` 函数 |

### 3.3 事件分发

| Pi 能力 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `subscribe(fn) → unsubscribe` | ✅ | `Agent.subscribe()` + broadcast channel |

### 3.4 Steering / Follow-up 队列

| Pi 能力 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `steer(m)` / `followUp(m)` | ✅ | |
| 按模式出队（All / OneAtATime） | ✅ | |
| `clearAllQueues` / `hasQueuedMessages` | ✅ | |

### 3.5 生命周期

| Pi 能力 | AgentDash 状态 | 说明 |
|---------|---------------|------|
| `prompt(message)` / `continue()` | ✅ | |
| `abort()` | ✅ | |
| `waitForIdle()` | ✅ | |
| `reset()` | ✅ | 清空 AgentState 全部字段 |
| 错误封装为 AssistantMessage | ✅ | `error_assistant()` |

---

## 四、不适用项（Bridge 层管理）

以下项目由 Rust 的 `LlmBridge` trait 在内部管理，无需在 Agent 层对齐：

- `model` 在 `AgentLoopConfig` 中
- `getApiKey`
- `streamFn` / `StreamFn`
- `sessionId` / `transport` / `maxRetryDelayMs`
- `proxy.ts` 全部（桌面应用不需要 HTTP 代理）
- `CustomAgentMessages` 扩展机制（当前无需求）
- `api` / `provider` / `model` assistant 元数据（需 Bridge 暴露后添加）

---

## 五、实施记录

| # | 变更 | 阶段 |
|---|------|------|
| 1 | `types.rs`: `ToolExecutionMode`、`BeforeToolCallResult/Context`、`AfterToolCallResult/Context` | P0 |
| 2 | `types.rs`: `AgentEvent` 对齐 — TurnEnd、MessageStart、ToolExecutionUpdate | P0 |
| 3 | `types.rs`: `AgentTool::execute` + `CancellationToken` | P0 |
| 4 | `types.rs`: `AgentError::ContinueError` | P0 |
| 5 | `agent_loop.rs`: `AgentLoopConfig` 全部钩子字段 | P0 |
| 6 | `agent_loop.rs`: 内外双循环 + steering 轮询 | P0 |
| 7 | `agent_loop.rs`: 三阶段工具执行 + 并行执行 | P0 |
| 8 | `agent_loop.rs`: `agent_loop_continue` 安全检查 | P0 |
| 9 | `agent_loop.rs`: prompt/tool result 消息事件 | P0 |
| 10 | `agent.rs`: `AgentConfig` 扩展 + `QueueMode` + 出队 | P0 |
| 11 | `bridge.rs`: `BridgeRequest.llm_messages` | P0 |
| 12 | `types.rs`: `StopReason`、`TokenUsage`、assistant 新字段 | P1 |
| 13 | `types.rs`: `error_assistant()` + `is_error_or_aborted()` | P1 |
| 14 | `agent.rs`: `is_running` + `idle_notify` + `wait_for_idle()` + `reset()` | P1 |
| 15 | `agent_loop.rs`: usage/stop_reason 传播 + stopReason 退出 | P1 |
| 16 | `types.rs`: timestamp、tool_name、details 字段 | P2 |
| 17 | `types.rs`: `AgentToolResult.details`、`AfterToolCallResult.details` | P2 |
| 18 | `types.rs`: `AgentTool::label()` + `on_update` + `ToolUpdateCallback` | P2 |
| 19 | `types.rs`: `ThinkingLevel` enum | P2 |
| 20 | `agent_loop.rs`: `build_on_update()` + details 透传 | P2 |
| 21 | `event_stream.rs`: mpsc → broadcast 多订阅者 | P2 |
| 22 | `agent.rs`: `subscribe()` + `set_thinking_level()` | P2 |
| 23 | `builtins.rs` + `pi_agent_mcp.rs`: execute 签名适配 | P2 |
| 24 | `types.rs`: `AgentState` struct — 统一可观测状态 | P3 |
| 25 | `types.rs`: `AssistantStreamEvent` — 流式子事件类型 | P3 |
| 26 | `types.rs`: `MessageDelta` → `MessageUpdate { message, event }` | P3 |
| 27 | `agent_loop.rs`: partial message 流式维护 + MessageUpdate 事件 | P3 |
| 28 | `agent.rs`: `Arc<Mutex<AgentState>>` 替代分散字段 | P3 |
| 29 | `agent.rs`: `process_event()` — 事件驱动状态同步 | P3 |
| 30 | `agent.rs`: `state()` / `try_state()` 状态快照访问 | P3 |
| 31 | `pi_agent.rs`: MessageUpdate 适配 + `replace_messages` await | P3 |
| 32 | `lib.rs`: 导出 `AgentState`、`AssistantStreamEvent`、`process_event` | P3 |
