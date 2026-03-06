# Rust 版 Pi × Rig SDK 缝合方案设计文档

文档时间：2026-03-06

相关本地仓库：
- `rig` @ `ac9033a6`
- `pi-mono` @ `b14c3592`

相关背景笔记：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\NOTES.md`

## 1. 前提与目标重述

在新增前提下，这个方案的目标需要重新收敛：

1. **不做 UI 层**
   - 不做 terminal UI
   - 不做 slash commands
   - 不做交互式产品壳
   - 外部系统会提供更完整的交互逻辑与产品体验

2. **目标是一个 SDK，而不是 coding-agent 产品**
   - SDK 需要能定义 session 信息
   - SDK 需要能运行完整 AgentLoop
   - SDK 需要在各个关键环节暴露可侵入点，方便接入业务逻辑、审计、策略、监控、流程编排

3. **能力层优先复用 Rig**
   - 环境连接
   - provider / model 接入
   - tool schema 与调用能力
   - 资源读取 / RAG / structured output / streaming
   - 尽量直接复用 Rig 及其成熟生态，而不是自己重做一层 provider/tool/resource 基础设施

4. **Pi 里最需要复刻的是 `pi-agent-core` 的 runtime 设计**
   - 双层消息模型
   - `transformContext` / `convertToLlm`
   - 事件驱动 AgentLoop
   - steering / follow-up 语义
   - runtime 级别的侵入点

因此，最终方向不是：

- Rust 重写 Pi Monorepo
- 或 Rig 替代 Pi

而是：

**用 Rig 承担能力层，用 Rust 复刻 Pi 的 AgentLoop 与 Session-Oriented SDK 抽象。**

## 2. 问题定义

我们要构建的，不是一个“会聊天的 CLI”，而是一个 **可嵌入外部系统的 Agent SDK**。

这个 SDK 需要解决三类问题：

### 2.1 能力问题

如何连接模型、工具、RAG、结构化输出、流式响应？

这部分尽量复用 Rig。

### 2.2 运行时问题

如何把“用户输入、系统注入消息、工具调用、上下文投影、模型响应、下一轮继续”组织成完整的 AgentLoop？

这部分重点复刻 Pi 的 `pi-agent-core`。

### 2.3 会话与侵入问题

如何让业务方在一个有状态的 session 中：

- 注入系统消息或外部上下文
- 修改上下文投影策略
- 中断或转向当前任务
- 在工具前后做权限、审计、埋点、策略控制
- 在每一轮、每个消息、每次工具执行阶段拿到回调事件
- 接管 session 元数据、持久化与恢复

这部分是原来 `pi-coding-agent` 中少量值得迁移的“协调壳”。

## 3. 非目标

以下内容不作为本方案的目标：

- terminal UI / web UI / desktop UI
- slash commands
- theme / keybinding / interactive widgets
- Pi 的 package manager
- Pi 的扩展市场与安装体系
- 完整复刻 `pi-coding-agent` 的产品能力
- 完整复刻 `pi-ai` 的 provider 兼容层
- TypeScript API 行为兼容

## 4. 高层结论

在新前提下，系统应收敛为三层：

1. **Rig Bridge 层**：复用 Rig 生态，提供统一能力接入
2. **Agent Runtime 层**：复刻 Pi 的 AgentLoop 核心语义
3. **Session SDK 层**：提供 session 定义、状态管理和侵入点编排

不再单独设计 UI / CLI 层。

## 5. 总体架构

### 5.1 三层架构

```text
+-----------------------------------------------------------+
| Session SDK Layer                                         |
| session definition, state, metadata, hooks, persistence,  |
| orchestration, policies, business intervention            |
+-----------------------------------------------------------+
| Agent Runtime Layer                                       |
| AgentMessage, transform_context, convert_to_llm,          |
| event lifecycle, tool loop, steering/follow-up            |
+-----------------------------------------------------------+
| Rig Bridge Layer                                          |
| provider/model/tool/resource/rag/schema/stream adapter    |
+-----------------------------------------------------------+
| Rig                                                        |
+-----------------------------------------------------------+
```

### 5.2 各层职责

#### Rig Bridge 层

职责：

- 适配 Rig 的 provider、model、tool、streaming、RAG、schema 能力
- 提供对 runtime 友好的统一接口
- 屏蔽 Rig 原生 API 细节

#### Agent Runtime 层

职责：

- 组织完整 AgentLoop
- 维护消息模型与 turn 语义
- 在每个阶段产生事件
- 执行工具循环
- 处理 steering / follow-up
- 暴露精细侵入点

#### Session SDK 层

职责：

- 定义 session 的输入、元数据和状态
- 暴露业务接入点
- 负责持久化 / 恢复 / session store
- 负责把 runtime 事件转成宿主系统可消费的 SDK 事件
- 负责策略、监控、审计、权限与业务流程对接

## 6. 复用边界：哪些直接用 Rig，哪些自己做

## 6.1 直接复用 Rig 的部分

在当前前提下，以下能力原则上都应该优先建立在 Rig 之上：

- provider / model 接入
- prompt / chat / completion / streaming
- tool schema 与工具调用基础机制
- 多轮 prompt request
- RAG / dynamic context
- structured output
- provider hook / tool-call hook
- 环境连接逻辑
- 基础资源接入能力

也就是说，**只要一个问题仍然属于“模型能力层”或“能力生态层”，优先交给 Rig。**

## 6.2 不应由 Rig 承担的部分

Rig 不是 Session-Oriented Agent SDK，它不天然提供以下 Pi 风格语义：

- `AgentMessage` 与 `LlmMessage` 分层
- `transform_context` / `convert_to_llm`
- runtime 事件流
- steering / follow-up 队列
- session 定义、元数据、状态恢复
- 业务侵入点编排
- 宿主系统可控的生命周期管理

这些必须在 Rust SDK 自己实现。

## 6.3 关于“工具执行”和“资源读取”的边界

你补充的前提非常关键：

- 很多业务能力层，包括 **环境连接、工具执行、资源读取** 都能复用 Rig 成熟生态

因此本方案中：

- Runtime **不再自己实现完整工具生态**
- Runtime **不再自己实现复杂资源加载系统**
- Runtime 只做：
  - 向 Rig Bridge 声明当前 request 的工具与资源约束
  - 在运行时捕获工具调用与工具结果事件
  - 提供工具调用前后可侵入点
  - 在 session 维度注入额外上下文

换句话说：

- **Rig 负责“怎么连、怎么调、怎么拿资源”**
- **SDK 负责“什么时候调、调之前之后怎么拦、调完如何进入下一轮”**

## 7. 核心设计：Agent Runtime 层

这部分是本方案真正的中心，应重点吸收 `pi-agent-core` 的思想。

## 7.1 设计目标

Agent Runtime 需要提供：

- 完整 AgentLoop
- 明确 turn 语义
- 双层消息模型
- 上下文投影与裁剪
- 工具调用循环
- 运行期 steering / follow-up
- 细粒度事件
- 多种侵入点

## 7.2 双层消息模型

建议保留 Pi 的核心设计，但以 Rust 类型系统重写。

```rust
pub enum AgentMessage {
    User(UserMessage),
    Assistant(AssistantMessage),
    ToolResult(ToolResultMessage),
    SystemNote(SystemNoteMessage),
    BusinessEvent(BusinessEventMessage),
    Artifact(ArtifactMessage),
    Custom(CustomMessage),
}

pub enum LlmMessage {
    User(...),
    Assistant(...),
    ToolResult(...),
}
```

设计意图：

- `AgentMessage` 面向 session 与业务系统
- `LlmMessage` 面向模型
- 并不是所有 `AgentMessage` 都需要进入模型
- 宿主业务可以定义自己的 runtime message 语义，而不污染模型上下文

## 7.3 上下文处理管线

保留 Pi 的两段式结构：

```text
AgentMessage[]
  -> transform_context()
  -> AgentMessage[]
  -> convert_to_llm()
  -> LlmMessage[]
  -> Rig Bridge
```

### 7.3.1 `transform_context()`

用途：

- 裁剪消息
- 压缩上下文
- 注入外部上下文
- 插入 session 级别说明
- 将宿主系统状态映射为 agent 级消息

### 7.3.2 `convert_to_llm()`

用途：

- 过滤 UI / 业务内部消息
- 把可转换消息投影成 LLM 消息
- 在最后一步决定真正送给模型的上下文

这是整个 SDK 可侵入性的第一核心。

## 7.4 AgentLoop 的运行语义

建议保留 Pi 的 loop 结构：

1. 添加输入消息
2. 发出 `agent_start` / `turn_start`
3. 对上下文做 transform 与 projection
4. 发起模型调用
5. 流式接收 assistant response
6. 识别 tool calls
7. 执行工具并记录 tool result
8. 检查 steering queue
9. 若有工具结果或 steering，继续下一轮
10. 若无更多动作，检查 follow-up queue
11. 收尾并发出 `turn_end` / `agent_end`

## 7.5 事件模型

SDK 需要暴露完整事件流，而不是只返回最终结果。

```rust
pub enum AgentEvent {
    AgentStart { session_id: String },
    AgentEnd { session_id: String, new_messages: Vec<AgentMessage> },
    TurnStart { turn_id: String },
    TurnEnd { turn_id: String, assistant: AssistantMessage, tool_results: Vec<ToolResultMessage> },
    MessageStart { message: AgentMessage },
    MessageUpdate { message: AgentMessage, delta: MessageDelta },
    MessageEnd { message: AgentMessage },
    ToolExecutionStart { tool_call_id: String, tool_name: String, args: serde_json::Value },
    ToolExecutionUpdate { tool_call_id: String, partial: ToolUpdate },
    ToolExecutionEnd { tool_call_id: String, result: ToolResultMessage },
    Intervention { phase: InterventionPhase, detail: String },
    Error { error: RuntimeError },
}
```

这个事件模型是宿主系统接入监控、审计、状态同步、日志和业务编排的核心。

## 7.6 steering / follow-up

这部分非常建议保留，因为它是 Pi loop 的高价值设计之一。

### steering

用于：

- 中断当前工作方向
- 在工具执行后插入新的优先消息
- 外部系统动态调整 agent 方向

### follow-up

用于：

- 当前任务自然结束后排队继续
- 后置任务处理
- 补充性消息延后送入

这对 SDK 尤其有价值，因为外部系统通常会有自己的事件流和任务编排器。

## 8. 核心设计：Session SDK 层

这是在新前提下，原来 `pi-coding-agent` 中真正需要迁移和抽象化的部分。

## 8.1 目标

Session SDK 层不是 UI 壳，而是：

- 定义 session
- 管理 session 状态
- 暴露宿主可控的侵入点
- 提供 session 生命周期 API
- 提供持久化抽象

## 8.2 Session 定义

建议把 session 抽象为可被宿主系统定义和序列化的对象。

```rust
pub struct AgentSessionDefinition {
    pub session_id: String,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub workflow_id: Option<String>,
    pub system_prompt: Option<String>,
    pub model_selector: ModelSelector,
    pub initial_messages: Vec<AgentMessage>,
    pub metadata: serde_json::Value,
}
```

### 关键点

- session 信息由宿主系统主导
- SDK 不强绑定特定产品字段
- 用 `metadata` 承载业务域特定信息

## 8.3 Session 状态

```rust
pub struct AgentSessionState {
    pub definition: AgentSessionDefinition,
    pub messages: Vec<AgentMessage>,
    pub pending_steering: Vec<AgentMessage>,
    pub pending_follow_up: Vec<AgentMessage>,
    pub active_turn: Option<ActiveTurn>,
    pub checkpoints: Vec<SessionCheckpoint>,
    pub extensions: serde_json::Value,
}
```

设计意图：

- 持有完整 runtime 视角下的会话态
- 支持恢复、回放、检查点与中断续跑
- 让宿主系统能在 session 维度做强控制

## 8.4 持久化抽象

不建议把 JSONL tree 作为首版唯一存储方案，而应先抽象 storage。

```rust
#[async_trait]
pub trait SessionStore {
    async fn load(&self, session_id: &str) -> Result<Option<AgentSessionState>, StoreError>;
    async fn save(&self, state: &AgentSessionState) -> Result<(), StoreError>;
    async fn append_event(&self, session_id: &str, event: &AgentEvent) -> Result<(), StoreError>;
}
```

### 为什么这样改

原本 Pi 的 JSONL tree 很适合产品化 shell，但你们当前目标是 SDK：

- 宿主系统可能已有数据库
- 宿主系统可能已有事件总线
- 宿主系统可能已有 workflow state store

因此 SDK 应该提供抽象而不是强绑定本地 session 文件格式。

### 是否还保留 tree session 思想

可以保留，但作为一种实现，而不是唯一规范：

- `JsonlTreeSessionStore`
- `SqlSessionStore`
- `EventSourcedSessionStore`

## 8.5 Session Facade

建议对外暴露一个面向宿主系统的高层入口。

```rust
pub trait AgentSdk {
    async fn create_session(&self, definition: AgentSessionDefinition) -> Result<AgentSessionHandle, SdkError>;
    async fn resume_session(&self, session_id: &str) -> Result<AgentSessionHandle, SdkError>;
}

pub trait AgentSessionHandle {
    async fn prompt(&mut self, input: SessionInput) -> Result<RunSummary, SdkError>;
    async fn continue_loop(&mut self) -> Result<RunSummary, SdkError>;
    async fn steer(&mut self, messages: Vec<AgentMessage>) -> Result<(), SdkError>;
    async fn enqueue_follow_up(&mut self, messages: Vec<AgentMessage>) -> Result<(), SdkError>;
    async fn snapshot(&self) -> Result<AgentSessionState, SdkError>;
}
```

这层本质上是把 `AgentLoop` 变成 session-oriented SDK API。

## 9. 关键设计：侵入点（Intervention Points）

你特别强调“我们需要 Pi 的 AgentLoop 来使得我们可以在各个有效环节处理我们的侵入”。

这句话决定了本方案的核心：

**SDK 的价值，不只是跑 loop，而是必须在 loop 各阶段允许宿主系统介入。**

## 9.1 必须提供的侵入点

建议至少暴露以下阶段：

### 会话级

- `on_session_created`
- `on_session_loaded`
- `on_session_persisting`
- `on_session_persisted`

### 输入级

- `before_input_accepted`
- `after_input_appended`

### 上下文级

- `before_transform_context`
- `after_transform_context`
- `before_convert_to_llm`
- `after_convert_to_llm`

### 模型调用级

- `before_model_request`
- `after_model_request`
- `on_stream_event`

### 工具级

- `before_tool_dispatch`
- `before_tool_execution`
- `after_tool_execution`
- `on_tool_error`

### Loop 控制级

- `before_turn_start`
- `after_turn_end`
- `before_steering_applied`
- `before_follow_up_applied`
- `before_loop_continue`
- `before_loop_finish`

### 错误与治理级

- `on_runtime_error`
- `on_policy_check`
- `on_guardrail_violation`

## 9.2 侵入点的实现形式

建议同时支持两种方式：

### Hook 风格

适合轻量策略和中间件：

```rust
#[async_trait]
pub trait RuntimeHook {
    async fn before_model_request(&self, ctx: &mut RuntimeContext) -> Result<(), HookError>;
    async fn after_tool_execution(&self, ctx: &mut RuntimeContext) -> Result<(), HookError>;
}
```

### Interceptor / Middleware 风格

适合更复杂的链式处理：

```rust
#[async_trait]
pub trait TurnInterceptor {
    async fn intercept(
        &self,
        ctx: TurnContext,
        next: NextTurn<'_>,
    ) -> Result<TurnResult, RuntimeError>;
}
```

### 选择建议

- 首版先做 Hook
- 如果确实出现复杂策略链，再扩展成 middleware

## 9.3 侵入点允许做什么

宿主系统应能通过侵入点实现：

- 增删上下文消息
- 注入业务状态
- 拒绝某个工具调用
- 改写模型参数
- 改写工具参数
- 触发审计日志
- 打埋点
- 触发审批流
- 对输出结果追加业务标签
- 触发错误恢复或 fallback

## 10. Rig Bridge 设计

## 10.1 目标

Rig Bridge 的目标不是复制 Rig，而是把 Rig 变成 Runtime 可稳定依赖的能力接口。

## 10.2 建议接口

```rust
pub trait RigBridge {
    type Stream: Stream<Item = BridgeEvent>;

    async fn stream(&self, request: BridgeRequest) -> Result<Self::Stream, BridgeError>;
    async fn complete(&self, request: BridgeRequest) -> Result<BridgeResponse, BridgeError>;
}
```

### `BridgeRequest` 应包含

- model selection
- llm messages
- tools exposure
- rag/resource selectors
- structured output schema
- runtime metadata

### `BridgeEvent` 应包含

- text delta
- thinking delta
- tool call start / delta / end
- done / error
- usage / stop reason

## 10.3 为什么不让 Runtime 直接调用 Rig

因为这样会有三个问题：

1. Runtime 会被 Rig API 细节污染
2. Runtime 事件模型无法稳定
3. 后续如果要支持别的连接层会很痛苦

因此 Bridge 是必要边界。

## 11. 迁移范围重新判断

在当前前提下，`pi-coding-agent` 真正需要迁移的只剩很少一部分。

## 11.1 核心必迁

- `pi-agent-core` 的 AgentLoop 设计
- session-oriented runtime facade
- runtime event system
- steering / follow-up 队列
- session store abstraction
- hook / intervention model

## 11.2 可以参考但不必强迁

- session tree 思想
- compaction 思想
- model resolver 的部分策略

这些可以保留设计灵感，但不需要照抄 `pi-coding-agent` 实现。

## 11.3 可以直接放弃迁移

- interactive mode
- slash commands
- prompt templates（若宿主系统已有更强 prompt orchestration）
- skills（若宿主系统已有资源装配方式）
- package manager
- extensions host
- theme / tui components

## 12. crate 划分建议

在 SDK-first 目标下，建议缩成四个 crate：

## 12.1 `rig_bridge`

职责：

- Rig 接入
- 能力适配
- provider/model/tool/resource/schema/stream 接口统一

## 12.2 `agent_core`

职责：

- `AgentMessage`
- `LlmMessage`
- AgentLoop
- turn 语义
- event stream
- steering/follow-up
- hooks / interventions

## 12.3 `session_sdk`

职责：

- session definition
- session state
- session facade API
- session persistence abstraction
- orchestration API

## 12.4 `session_store_impls`

职责：

- JSONL / SQL / event-store 等 session store 实现
- 可按宿主系统选择接入

如果未来需要，再加：

- `sdk_observability`
- `sdk_policies`
- `sdk_compaction`

## 13. 首版开发阶段建议

## 13.1 Phase 0：打通最小链路

交付：

- `rig_bridge`
- `agent_core` 最小 loop
- 一次 prompt + streaming
- 单工具循环
- 基础 hooks

## 13.2 Phase 1：补全 runtime 语义

交付：

- `AgentMessage` / `LlmMessage`
- `transform_context`
- `convert_to_llm`
- 事件模型
- steering / follow-up
- 完整工具循环

## 13.3 Phase 2：补全 session SDK

交付：

- session definition
- session state
- session facade
- session store abstraction
- persist / resume
- intervention points

## 13.4 Phase 3：补全企业接入能力

交付：

- metrics / tracing
- 审计与策略 hooks
- 多 store 实现
- fallback / retry 策略
- compaction（如果确实需要）

## 14. 首版验收标准

满足以下条件即可认为首版成功：

- 能通过 Rig 跑完整多轮 AgentLoop
- 有 Pi 风格双层消息模型
- 有 `transform_context` / `convert_to_llm`
- 有清晰事件流
- 有 steering / follow-up
- 有 session definition / state / resume
- 有 session store abstraction
- 宿主系统可在关键阶段侵入
- 工具执行、资源读取、环境连接不需要在 SDK 内重造

## 15. 最终建议

在新前提下，这个项目不应该再被理解为“Rust 版 coding-agent”，而应该被定义为：

**一个基于 Rig 能力底座、复刻 Pi AgentLoop 精华、面向宿主系统可侵入的 Session-Oriented Agent SDK。**

更具体地说：

- **Rig 负责能力和生态**
- **`agent_core` 负责 loop 与 runtime 语义**
- **`session_sdk` 负责对宿主系统暴露 session 与侵入点**

这会比“迁移 `pi-coding-agent`”更轻、更准，也更符合你现在的目标。

## 16. 一句话总结

**在这个版本里，真正要复刻的是 Pi 的 AgentLoop；真正要复用的是 Rig 的能力层；最终交付物是 SDK，而不是 UI 产品。**
