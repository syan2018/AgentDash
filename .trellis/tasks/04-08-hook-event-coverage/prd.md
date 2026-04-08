# Hook 事件体系补全

> 状态：planning
> 参考：`references/pi-mono/packages/coding-agent/src/core/extensions/types.ts`

## 背景

我们当前的 `HookTrigger` 覆盖了 agent 生命周期的主干：
`SessionStart` / `UserPromptSubmit` / `BeforeTool` / `AfterTool` / `AfterTurn` / `BeforeStop` / `SessionTerminal` / `BeforeSubagentDispatch` / `AfterSubagentDispatch` / `SubagentResult`

pi-coding-agent 的 extension 系统还有若干我们缺失的事件类型和 Plugin 注册能力，对以下场景有用：

| 缺失能力 | 典型场景 |
|---------|---------|
| `input` 事件 | 拦截用户输入做安全过滤/自动扩写 |
| `before_agent_start` | 每轮动态注入当前时间/环境变量到 system prompt |
| `before_provider_request` | 观测/改写发往 LLM 的完整 payload（provider 透明代理） |
| Plugin `registerCommand()` | Extension 注册自定义 slash 命令 |
| Plugin `registerFlag()` | Extension 注册运行时布尔/字符串 flag |
| `CustomMessage<T>` | Extension 向对话注入自定义载荷消息 |

## 设计

### 1. 新增 HookTrigger 类型

```rust
// agentdash-spi/src/hooks.rs
pub enum HookTrigger {
    // 已有...

    /// 用户输入抵达 agent 之前，可 transform 或 block
    UserInput,

    /// 每轮 agent 启动前，可注入消息或追加 system prompt 片段
    BeforeAgentStart,

    /// LLM API 请求发出前，可观测/改写完整 payload
    BeforeProviderRequest,
}
```

### 2. `UserInput` 事件

**触发时机**：`start_prompt_with_follow_up()` 接收到用户消息后、提交给 agent loop 前。

**Hook 输入**：
```rust
pub struct UserInputPayload {
    pub message: String,
    pub session_id: String,
}
```

**Hook 输出**（扩展 `HookResolution`）：
```rust
pub struct UserInputResolution {
    pub block: bool,
    pub block_reason: Option<String>,
    pub transformed_message: Option<String>,  // 改写后的消息
}
```

**典型规则**：屏蔽包含敏感词的输入、自动在输入前追加项目上下文模板。

### 3. `BeforeAgentStart` 事件

**触发时机**：每次调用 `AgentLoopConfig` 开始新的 prompt 处理前（含 follow-up 轮）。

**Hook 输出**（扩展 `HookInjection`）：
```rust
pub struct BeforeAgentStartResolution {
    pub system_prompt_append: Option<String>,  // 追加到 system prompt 末尾
    pub inject_messages: Vec<AgentMessage>,    // 注入为 steering messages
}
```

**典型规则**：每轮注入当前 UTC 时间、git branch 名、运行环境标签。

### 4. `BeforeProviderRequest` 事件

**触发时机**：`rig_bridge` 在调用 LLM provider 前（`stream_complete` 之前）。

**Hook 输入**：
```rust
pub struct ProviderRequestPayload {
    pub model: String,
    pub messages: serde_json::Value,  // 序列化的 LLM messages
    pub tools: serde_json::Value,
    pub system: Option<String>,
}
```

**Hook 输出**：
```rust
pub struct ProviderRequestResolution {
    pub override_payload: Option<serde_json::Value>,  // 完整替换 payload
}
```

**典型用途**：LLM 请求日志、token 预算追踪、payload 调试。注意：此 hook 不应做 block，只做观测或幂等改写。

### 5. Plugin `registerCommand()`

Plugin API 新增注册 slash 命令的能力：

```rust
// agentdash-plugin-api
pub trait PluginCommandProvider {
    fn commands(&self) -> Vec<PluginCommand>;
}

pub struct PluginCommand {
    pub name: String,           // 不含斜杠，如 "my-plugin:reset"
    pub description: String,
    pub handler: PluginCommandHandler,
}

pub enum PluginCommandHandler {
    InjectMessage(String),          // 直接注入固定消息
    TriggerHook(HookTrigger),       // 触发指定 hook
}
```

注册的命令与 skill `/skill:` 命令通过同一套 slash command 路由分发。

### 6. Plugin `registerFlag()`

```rust
pub struct PluginFlag {
    pub name: String,                   // 如 "my-plugin.verbose"
    pub flag_type: PluginFlagType,      // Bool | String
    pub default: serde_json::Value,
    pub description: String,
}
```

Flag 值在 session 生命周期内存活，可被 hook 规则读取（通过 `HookSessionState` 或单独的 flag store）。

### 7. `CustomMessage<T>` 通用 Extension 消息类型

在 `AgentMessage` 消息体系中新增 `role: "extension"` 类型：

```rust
pub struct ExtensionMessage {
    pub extension_id: String,
    pub custom_type: String,
    pub content: serde_json::Value,
    pub display: Option<String>,       // 可选的人类可读摘要
    pub exclude_from_context: bool,    // true 时不送给 LLM
}
```

Extension 通过 `session.inject_extension_message()` 写入，前端通过 custom_type 路由到对应渲染组件。

## 实施顺序建议

1. `UserInput` + `BeforeAgentStart`（改动集中在 `prompt_pipeline.rs` 和 `agent_loop.rs`）
2. `BeforeProviderRequest`（改动集中在 `rig_bridge.rs`）
3. `registerCommand()` + `registerFlag()`（Plugin API + slash command 路由）
4. `CustomMessage<T>`（消息类型扩展 + 前端渲染路由）

## 与现有 03-30-hook-external-triggers 的关系

`03-30-hook-external-triggers` 专注于：
- Session 状态容器（`HookSessionState`）
- 外部系统触发入口（`ExternalMessage` / `StateChange` / `ConditionMet`）

本任务专注于：
- Agent 内部生命周期的缺失事件（`UserInput` / `BeforeAgentStart` / `BeforeProviderRequest`）
- Plugin API 层的注册能力扩展

两者不重叠，可并行实施。
