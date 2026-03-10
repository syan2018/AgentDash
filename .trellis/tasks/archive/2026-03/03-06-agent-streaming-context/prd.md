# Agent 流式输出与上下文管理

## 1. 背景与目标

当前 `agentdash-agent` 的 `RigBridge` 使用 Rig 的 `completion()` API 进行 LLM 调用，这是一个阻塞式的完整响应模式——用户必须等待整个回复生成完毕才能看到内容。对于长回复或复杂推理，这导致了不可接受的用户体验延迟。

同时，Agent 的上下文管理（对话历史、Token 窗口控制）也需要完善，以支持多轮对话和长会话场景。

本任务的目标是：
1. 将 `RigBridge` 改为流式输出（token-by-token streaming）
2. 在 `PiAgentConnector` 中将流式 token 转换为 ACP `MessageDelta` 通知
3. 实现对话历史管理和 Token 窗口控制
4. 支持多轮对话的上下文连续性

## 2. 当前约束

1. `LlmBridge` trait 定义了 `complete()` 方法，返回 `LlmResponse { content, tool_calls }`——非流式。
2. Rig 支持流式 API（`stream_completion()`），但 `RigBridge` 未使用。
3. `AgentEvent` 已定义 `MessageChunk(String)` 事件类型，但未被使用。
4. `PiAgentConnector` 通过 `EventReceiver` 消费 `AgentEvent`，转换为 ACP `SessionNotification`。
5. ACP 协议已支持 `SessionUpdate::MessageDelta` 用于流式文本。
6. `Agent` 的 `messages` 字段（`Vec<AgentMessage>`）持有当前对话历史，但无 Token 计数和窗口控制。

## 3. Goals / Non-Goals

### Goals

- **G1**: `LlmBridge` 支持流式输出——新增 `stream_complete()` 方法
- **G2**: `RigBridge` 实现 `stream_complete()`，逐 token 通过 channel 发送
- **G3**: `agent_loop` 使用流式调用，边生成边发送 `AgentEvent::MessageChunk`
- **G4**: `PiAgentConnector` 将 `MessageChunk` 转换为 ACP `MessageDelta`
- **G5**: 对话历史 Token 窗口管理——当历史超过阈值时智能截断
- **G6**: 多轮对话上下文保持——同一 session 的后续 prompt 继承之前的对话历史

### Non-Goals

- 不做 Token 精确计数（使用近似估算，如 chars/4）
- 不做对话历史持久化到数据库（当前内存 + session file 已足够）
- 不做对话摘要/压缩（后续功能）
- 不做多模态流式（图片等）

## 4. ADR-lite（核心决策）

### 决策 A：LlmBridge 增加 stream_complete 方法

在 trait 上新增 `stream_complete()`，返回 `Pin<Box<dyn Stream<Item = StreamChunk>>>>`。

```rust
pub enum StreamChunk {
    TextDelta(String),
    ToolCallStart { id: String, name: String },
    ToolCallInputDelta { id: String, delta: String },
    ToolCallEnd { id: String },
    Done { stop_reason: Option<String> },
    Error(String),
}
```

保留原有的 `complete()` 作为 fallback（某些 provider 可能不支持流式）。

### 决策 B：agent_loop 优先使用流式

`agent_loop` 调用 `bridge.stream_complete()` 如果可用，否则 fallback 到 `complete()`。
流式调用时：
1. 逐个 `TextDelta` 发送 `AgentEvent::MessageChunk`
2. 所有 delta 聚合为完整 content 后，再处理 tool_calls
3. Tool call 参数通过 `ToolCallInputDelta` 逐块传入，最终聚合为完整 JSON

### 决策 C：Token 窗口使用 chars/4 近似估算

```rust
fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4
}
```

当历史 token 超过 `max_context_tokens`（默认 100k）时：
1. 保留 system prompt（始终）
2. 保留最近 N 轮（至少最近 2 轮）
3. 从最早的轮次开始丢弃，直到满足窗口限制

### 决策 D：多轮对话通过 Agent 实例持久化

`PiAgentConnector` 为每个 session 维持一个 `Agent` 实例（`HashMap<String, Agent>`）。
同一 session 的后续 prompt 复用同一 Agent 实例，自动继承对话历史。
Session 删除时清理对应 Agent 实例。

## 5. Signatures

### 5.1 LlmBridge 扩展

```rust
// agentdash-agent/src/bridge.rs

pub enum StreamChunk {
    TextDelta(String),
    ToolCallStart { id: String, name: String },
    ToolCallInputDelta { id: String, delta: String },
    ToolCallEnd { id: String },
    Done { stop_reason: Option<String> },
    Error(String),
}

#[async_trait]
pub trait LlmBridge: Send + Sync {
    async fn complete(
        &self,
        messages: &[rig::Message],
        tools: &[serde_json::Value],
        system: Option<&str>,
    ) -> Result<LlmResponse, AgentError>;
    
    /// 流式补全（默认实现：fallback 到 complete）
    fn stream_complete(
        &self,
        messages: Vec<rig::Message>,
        tools: Vec<serde_json::Value>,
        system: Option<String>,
    ) -> Pin<Box<dyn Stream<Item = StreamChunk> + Send>>;
    
    /// 是否支持原生流式
    fn supports_streaming(&self) -> bool { false }
}
```

### 5.2 RigBridge 流式实现

```rust
// agentdash-agent/src/bridge.rs
impl LlmBridge for RigBridge {
    fn supports_streaming(&self) -> bool { true }
    
    fn stream_complete(
        &self,
        messages: Vec<rig::Message>,
        tools: Vec<serde_json::Value>,
        system: Option<String>,
    ) -> Pin<Box<dyn Stream<Item = StreamChunk> + Send>> {
        // 使用 Rig 的 stream API
        // 将 Rig stream events 映射为 StreamChunk
    }
}
```

### 5.3 AgentEvent 扩展

```rust
// agentdash-agent/src/types.rs
pub enum AgentEvent {
    // 现有...
    MessageChunk(String),         // 已定义但未使用
    ToolCallStarted { ... },      // 已定义
    ToolCallCompleted { ... },    // 已定义
    
    // 新增
    ToolCallInputDelta {          // 工具参数增量
        tool_call_id: String,
        delta: String,
    },
}
```

### 5.4 上下文窗口管理

```rust
// agentdash-agent/src/context_window.rs (NEW)
pub struct ContextWindowManager {
    max_tokens: usize,
    reserved_for_response: usize,
}

impl ContextWindowManager {
    pub fn new(max_tokens: usize) -> Self;
    
    /// 对消息历史进行截断，确保在 token 限制内
    pub fn truncate_history(
        &self,
        system_prompt: &str,
        messages: &[AgentMessage],
    ) -> Vec<AgentMessage>;
    
    /// 估算 token 数
    pub fn estimate_tokens(text: &str) -> usize;
    
    /// 估算消息列表的总 token 数
    pub fn estimate_messages_tokens(messages: &[AgentMessage]) -> usize;
}
```

### 5.5 PiAgentConnector 会话管理

```rust
// agentdash-executor/src/connectors/pi_agent.rs
pub struct PiAgentConnector {
    workspace_path: PathBuf,
    bridge: Arc<dyn LlmBridge>,
    system_prompt: String,
    
    // 新增：session → Agent 实例映射
    sessions: Arc<RwLock<HashMap<String, Agent>>>,
}

impl PiAgentConnector {
    /// 获取或创建 session 对应的 Agent
    fn get_or_create_agent(&self, session_id: &str) -> Agent;
    
    /// 清理 session
    fn remove_session(&self, session_id: &str);
}
```

## 6. Contracts

### 6.1 流式输出 → ACP MessageDelta 契约

```
StreamChunk::TextDelta("Hello") 
  → ACP SessionNotification::Update(MessageDelta { content: "Hello" })

StreamChunk::ToolCallStart { id, name }
  → ACP SessionNotification::Update(ToolCall { id, name, status: "started" })

StreamChunk::Done
  → ACP SessionNotification::Update(Finished)
```

### 6.2 上下文窗口截断契约

截断时的日志输出：
```
[WARN] Context window exceeded: {current_tokens} > {max_tokens}. 
       Truncating {dropped_turns} turns ({dropped_tokens} tokens).
       Remaining: {remaining_turns} turns ({remaining_tokens} tokens).
```

### 6.3 多轮对话契约

同一 session 的连续 prompt 调用：
```
prompt("你好") → Agent(session_1) created → history: [user: "你好", assistant: "..."]
prompt("继续上面的话题") → Agent(session_1) reused → history: [user: "你好", assistant: "...", user: "继续上面的话题", assistant: "..."]
```

## 7. Validation & Error Matrix

| 场景 | 处理方式 |
|------|----------|
| Stream 中途断开 | 发送 Error chunk → 前端显示部分内容 + 错误提示 |
| Token 窗口不足以保留最近 2 轮 | 警告日志，仅保留最后 1 轮 |
| Agent 实例内存泄漏 | Session 删除时清理，服务重启时全部清空 |
| LLM Provider 不支持流式 | Fallback 到 complete()，一次性发送完整内容 |

## 8. Good / Base / Bad Cases

### Good
- 用户发送 prompt → 看到文本逐字出现（<100ms 延迟开始）→ 工具调用实时显示
- 多轮对话自然延续，Agent 记住上下文
- 长对话自动截断早期历史，保持响应质量

### Base
- LLM 不支持流式 → 用户等待完整响应后一次性显示
- 单轮对话（不需要历史管理）

### Bad
- Stream 中途 LLM 报错 → 显示已生成内容 + 错误信息 → 用户可重试

## 9. 验收标准

- [ ] `LlmBridge` 新增 `stream_complete()` 方法
- [ ] `RigBridge` 实现流式补全，逐 token 返回 `StreamChunk`
- [ ] `agent_loop` 优先使用流式调用
- [ ] `AgentEvent::MessageChunk` 被正确发送
- [ ] `PiAgentConnector` 将 chunk 转换为 ACP `MessageDelta`
- [ ] 前端会话页面逐字显示回复内容
- [ ] `ContextWindowManager` 正确截断超长历史
- [ ] 同一 session 多轮对话保持上下文
- [ ] Session 删除时清理 Agent 实例
- [ ] Fallback：非流式 provider 仍然正常工作

## 10. 实施拆分（建议）

### Phase 1: LlmBridge 流式支持（约 2h）
1. `StreamChunk` 类型定义
2. `LlmBridge::stream_complete()` trait 方法
3. `RigBridge` 流式实现（对接 Rig stream API）

### Phase 2: agent_loop 流式集成（约 2h）
4. agent_loop 改用 stream_complete
5. MessageChunk 事件发送
6. Tool call streaming 处理

### Phase 3: PiAgentConnector 流式转换（约 1.5h）
7. MessageChunk → ACP MessageDelta 转换
8. 会话管理（Agent 实例持久化）
9. Session 清理

### Phase 4: 上下文窗口（约 1.5h）
10. ContextWindowManager 实现
11. 截断策略
12. 集成到 Agent.prompt()

## 11. 依赖

- **Rig stream API**：需确认 rig-core 是否支持 streaming（需要查阅文档）
- **agent-tool-system** (03-06)：工具调用的流式 delta 需要工具系统已就绪
- **ACP MessageDelta**：确认 agent-client-protocol 已定义此类型
