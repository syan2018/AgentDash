# LLM 模型配置跨层契约

> 本规范描述 AgentDash 中模型选择、推理级别、provider 管理的跨层契约。
> 参考来源：`references/pi-mono/packages/agent/src/types.ts`（AgentState）

---

## 1. 核心原则

### 业务层只关心三件事

对齐 pi-mono `AgentState`，Agent 运转只需要：

| 参数 | 类型 | 说明 |
|------|------|------|
| `model_id` | `String` | 使用的模型 ID |
| `provider_id` | `String` | 所属 provider（如 `anthropic`、`gemini`） |
| `thinking_level` | `ThinkingLevel` | 推理强度（`off/minimal/low/medium/high/xhigh`） |

**已删除（不再暴露给业务层）**：`temperature`、`max_tokens`、`top_p`、`top_k`、`budget_tokens`

---

## 2. ThinkingLevel 枚举契约

### 后端定义

```rust
// crates/agentdash-executor/src/connector.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}
```

序列化值（前后端完全一致）：`"off"` / `"minimal"` / `"low"` / `"medium"` / `"high"` / `"xhigh"`

### 前端定义

```ts
// frontend/src/types/index.ts
export type ThinkingLevel = 'off' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh';
```

### VibeKanban 路径兼容

VibeKanban 仍使用字符串 `reasoning_id`。转换在 `to_vibe_kanban_config()` 中完成：

```rust
reasoning_id: self.thinking_level.map(|level| match level {
    ThinkingLevel::Off => "off",
    ThinkingLevel::Minimal => "minimal",
    // ...
}.to_string()),
```

---

## 3. AgentDashExecutorConfig 契约

```rust
// crates/agentdash-executor/src/connector.rs
pub struct AgentDashExecutorConfig {
    pub executor: String,
    pub variant: Option<String>,
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub thinking_level: Option<ThinkingLevel>,   // ← 替代旧的 reasoning_id: Option<String>
    pub permission_policy: Option<String>,
}
```

序列化为 HTTP JSON（`snake_case`）：
```json
{
  "executor": "PI_AGENT",
  "model_id": "claude-opus-4-6",
  "thinking_level": "high"
}
```

---

## 4. ModelInfo 元数据契约

`discover_options_stream` 返回的模型列表中，每个模型包含：

### 后端输出（JSON patch 中的 value）

```json
{
  "id": "claude-opus-4-6",
  "name": "Claude Opus 4.6",
  "provider_id": "anthropic",
  "reasoning": true,
  "context_window": 200000,
  "max_tokens": 32000
}
```

### 前端类型

```ts
// frontend/src/features/executor-selector/model/types.ts
export interface ModelInfo {
  id: string;
  name: string;
  provider_id: string;
  reasoning: boolean;       // 是否支持 extended thinking
  context_window: number;   // 上下文窗口大小（tokens）
  max_tokens: number;       // 最大输出 tokens
}
```

**关键约束**：`reasoning: true` 的模型才在 UI 中显示 ThinkingLevel 选择器。

---

## 5. PiAgent Provider Registry 架构

### 已支持的 Provider

| provider_id | 服务商 | API Key 设置 key | 备注 |
|-------------|--------|-----------------|------|
| `anthropic` | Anthropic Claude | `llm.anthropic.api_key` | 优先级最高 |
| `gemini` | Google Gemini | `llm.gemini.api_key` | 支持 thinking |
| `deepseek` | DeepSeek | `llm.deepseek.api_key` | R1 支持 reasoning |
| `groq` | Groq | `llm.groq.api_key` | 高速推理 |
| `xai` | xAI (Grok) | `llm.xai.api_key` | Grok Mini 支持 thinking |
| `openai` | OpenAI / 兼容端点 | `llm.openai.api_key` | 支持自定义 base_url |

### 注册优先级

构建顺序决定默认 provider（第一个注册的为默认）：
`anthropic → gemini → deepseek → groq → xai → openai`

### 运行时模型 override

`prompt()` 中通过 `find_bridge_for_model(model_id)` 找到对应 provider 的 bridge 工厂，动态创建 bridge。若未找到则回退到默认 bridge。

---

## 6. 数据流

```
前端 ExecutorSelector 选择模型
  ↓ { executor: "PI_AGENT", model_id: "claude-opus-4-6", thinking_level: "high" }
  ↓ POST /sessions/{id}/prompt
后端 resolve_task_executor_config()
  ↓ AgentDashExecutorConfig { model_id, thinking_level }
PiAgentConnector.prompt()
  ↓ find_bridge_for_model(model_id) → Arc<dyn LlmBridge>
  ↓ agent.set_thinking_level(thinking_level)
AgentLoop → BridgeRequest → LLM API
```

---

## 7. Forbidden Patterns

```
❌ 在 AgentConfig 中暴露 temperature/max_tokens 给外部配置
❌ 用 reasoning_id: String 表示推理强度（改用 ThinkingLevel 枚举）
❌ 构建时硬编码 model（如 CLAUDE_4_SONNET）而非运行时按 model_id 选择
❌ 前端下拉选项依赖后端 reasoning_options 列表（改为前端固定枚举）
```

---

## 8. Settings Key 命名规范

```
llm.anthropic.api_key
llm.gemini.api_key
llm.deepseek.api_key
llm.groq.api_key
llm.xai.api_key
llm.openai.api_key
llm.openai.base_url          # 可选，OpenAI 兼容端点
llm.openai.default_model     # 可选，默认模型 ID
llm.ollama.base_url          # 本地 Ollama，无 api_key
agent.pi.system_prompt       # PiAgent 系统提示词
```
