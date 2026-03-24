# 扩展 PiAgent LLM Provider 支持 + 前端发现链路整治

## 背景

当前 PiAgent（自研 Agent 链路）只支持两个 LLM provider：
- `anthropic`（Anthropic Claude，API Key 硬编码为 CLAUDE_4_SONNET）
- `openai`（OpenAI 兼容，支持自定义 base_url）

而 Rig 框架（`rig-core v0.24`）已内置支持 16+ provider，扩展成本极低（共用同一个 `RigBridge<M>` 泛型，只需创建对应 client）。

此外，前端的 LLM 服务发现与展示链路存在以下问题：
1. **Provider 列表缺失前端展示**：前端 ExecutorSelector 只显示 model 下拉，没有 provider 层级
2. **PiAgent 发现始终只返回单个 model**：用户无法感知当前配置了哪个 model，也无法切换
3. **Settings 中无 provider 管理 UI**：只有 OpenAI/Anthropic 两个硬编码的 API Key 输入框，无法添加新 provider
4. **Model 元数据（contextWindow/maxTokens/reasoning）缺失**：前端 `ModelInfo` 里这些字段未传递，导致 UI 无法判断"是否支持 thinking"

## Goal

1. 在 PiAgent 后端支持更多 Rig 内置 provider：**DeepSeek**、**Google Gemini**、**Ollama（本地）**、**Groq**、**xAI（Grok）**、**OpenRouter**
2. PiAgent 的 `discover_options_stream` 改为动态：根据 settings 中已配置 API Key 的 provider，返回对应的真实模型列表（含 `reasoning`、`contextWindow`、`maxTokens` 元数据）
3. 前端 Settings 页新增 "LLM Providers" 管理区：可视化各 provider 的配置状态（是否有 key）
4. 前端 `ModelInfo` 类型补充 `reasoning: boolean`、`context_window: number`、`max_tokens: number` 字段，用于驱动 ThinkingLevel 选择器的显示/隐藏逻辑

## Requirements

### Phase 0：后端模型元数据扩展（前置，为后续奠基）

- [ ] **`ModelInfo` 后端 DTO 增加元数据字段**
  - 文件：`crates/agentdash-executor/src/connector.rs` 中的 `ModelInfo` 结构体（或等效类型）
  - 增加字段：`reasoning: bool`、`context_window: u64`、`max_tokens: u64`、`input_modalities: Vec<String>`
  - PiAgent `discover_options_stream` 返回时填充这些字段

- [ ] **后端维护各 provider 的模型元数据表**
  - 在 `crates/agentdash-executor/src/connectors/pi_agent.rs` 中维护一个静态模型表（参考 pi-mono `packages/ai/src/types.ts` 的模型定义）
  - 至少覆盖各 provider 的主力模型（Anthropic: claude-opus-4-6, claude-sonnet-4-6; Gemini: gemini-2.5-pro; DeepSeek: deepseek-chat, deepseek-reasoner 等）
  - `reasoning: true` 的模型：claude-opus-4-6, claude-sonnet-4-6, deepseek-reasoner, gemini-2.5-pro（及后续）

### Phase 1：后端新增 Provider 支持

针对每个新 provider，在 `build_pi_agent_connector`（或重构为 provider registry）中添加构建逻辑：

- [ ] **DeepSeek**
  - Settings key：`llm.deepseek.api_key`，默认 base URL `https://api.deepseek.com`
  - 模型：`deepseek-chat`（默认）、`deepseek-reasoner`（支持 reasoning）
  - Rig：`rig::providers::deepseek::Client::new(key).completion_model(model_id)`
  - `discover_options_stream` 返回 deepseek provider + 2 个模型

- [ ] **Google Gemini**
  - Settings key：`llm.gemini.api_key`
  - 模型：`gemini-2.5-pro`（支持 thinking）、`gemini-2.0-flash`
  - Rig：`rig::providers::gemini::Client::new(key).completion_model(model_id)`

- [ ] **Ollama（本地，无需 API Key）**
  - Settings key：`llm.ollama.base_url`（默认 `http://localhost:11434`）
  - 模型列表：通过调用 `GET {base_url}/api/tags` 动态获取（Ollama API）
  - `discover_options_stream` 异步拉取本地模型列表后返回

- [ ] **Groq**
  - Settings key：`llm.groq.api_key`
  - 模型：`llama-3.3-70b-versatile`、`llama3-8b-8192` 等
  - Rig：`rig::providers::groq::Client::new(key).completion_model(model_id)`

- [ ] **xAI（Grok）**
  - Settings key：`llm.xai.api_key`
  - 模型：`grok-3`、`grok-3-mini`（支持 reasoning）
  - Rig：`rig::providers::xai::Client::new(key).completion_model(model_id)`

- [ ] **OpenRouter（聚合路由，低优先级）**
  - Settings key：`llm.openrouter.api_key`
  - 模型：通过 `GET https://openrouter.ai/api/v1/models` 动态获取
  - Rig：`rig::providers::openrouter::Client::new(key).completion_model(model_id)`

- [ ] **重构 `build_pi_agent_connector` 为多 provider 架构**
  - 不再构建时绑定单一 provider，而是构建一个 provider registry（`HashMap<provider_id, Arc<dyn LlmBridge>>`）
  - `prompt()` 时根据 `executor_config.provider_id` + `executor_config.model_id` 查找对应 bridge
  - 回退：若 `provider_id` 为空，按 settings 中优先级（anthropic > gemini > deepseek > openai > groq > xai > ollama）选首个已配置 provider

### Phase 2：PiAgent discover_options 动态化

- [ ] **`discover_options_stream` 改为动态返回**
  - 文件：`crates/agentdash-executor/src/connectors/pi_agent.rs`
  - 当前：始终返回硬编码的单一 provider + 单一 model
  - 改为：遍历所有已配置 settings key 的 provider，各自产出对应的 provider 定义 + 模型列表（含元数据）
  - Ollama 模型列表异步获取，先推送其他 provider，Ollama 加载完成后再 patch 追加
  - `supports_model_override` 改为 `true`

- [ ] **增加 `provider_id` 字段到 `ExecutorDiscoveredOptions`**（如现在不存在）
  - 与前端 `ModelInfo.provider_id` 对应

### Phase 3：前端发现链路整治

- [ ] **`ModelInfo` 类型补充元数据字段**
  - 文件：`frontend/src/features/executor-selector/model/types.ts`
  - 增加：`reasoning: boolean`、`context_window: number`、`max_tokens: number`

- [ ] **ExecutorSelector：ThinkingLevel 选择器显示逻辑**
  - 当选中的 `ModelInfo.reasoning === true` 时，才显示 ThinkingLevel 选择器
  - 否则隐藏（不支持 thinking 的模型不展示无效选项）

- [ ] **Settings 页新增 LLM Providers 管理区**
  - 文件：`frontend/src/pages/SettingsPage.tsx`
  - 当前：仅 OpenAI/Anthropic 两个硬编码 API Key 输入框
  - 改为：统一的 provider 列表，每个 provider 显示：
    - Provider 名称 + Logo（可选）
    - API Key 输入（脱敏展示）
    - 已配置状态指示（✅ / 未配置）
    - 自定义 Base URL 输入（仅支持该字段的 provider）
  - Provider 列表：Anthropic、Gemini、DeepSeek、OpenAI（兼容）、Groq、xAI、Ollama（base_url）、OpenRouter

- [ ] **前端 `ExecutorConfig` 增加 `provider_id` 字段**
  - 文件：`frontend/src/services/executor.ts`
  - `frontend/src/types/index.ts`
  - 将 provider 选择从 model 选择中解耦（当前 provider 只能靠 model 的 provider_id 反推）

- [ ] **Agent Preset Editor 中 model_id 输入改为下拉**
  - 文件：`frontend/src/features/project/agent-preset-editor.tsx`
  - 当前：自由文本输入框
  - 改为：复用 `discovered-options` 流数据的 model selector（等同于 ExecutorSelector 的 model 下拉）

## Acceptance Criteria

- [ ] Settings 页可以配置 Anthropic / Gemini / DeepSeek / Groq / xAI / Ollama / OpenRouter 的 API Key
- [ ] 各 provider 的 API Key 已配置时，ExecutorSelector 中能看到对应 provider 及其模型（通过 discover_options 流）
- [ ] Ollama 本地模型能在 ExecutorSelector 中动态列出
- [ ] 选择支持 reasoning 的模型（如 claude-opus-4-6）时才显示 ThinkingLevel 选择器
- [ ] 选择不支持 reasoning 的模型（如 gpt-5.4、llama-3.3-70b）时 ThinkingLevel 选择器消失
- [ ] Agent Preset Editor 中 model 选择是下拉而非文本框
- [ ] PiAgent 的 `supports_model_override` 为 true，用户在 ExecutorSelector 切换模型后实际生效

## Technical Notes

### Rig v0.24 Provider 可用性

工作区使用 `rig-core = "0.24"`，需确认该版本中哪些 provider 已存在。优先支持 `0.24` 中已有的 provider，若某个 provider 仅在 v0.32 中存在，需决策是否通过 `[patch.crates-io]` 升级到本地 v0.32 版本（`third_party/rig`）。

### Provider Registry 架构

```rust
struct PiAgentConnector {
    // 新：按 provider_id 存储已配置的 client
    providers: HashMap<String, Arc<dyn LlmBridge>>,  // provider_id → bridge factory
    default_provider: Option<String>,
    // 旧字段的 model_id 保留为 "default model ID"
    model_id: String,
}

impl AgentConnector for PiAgentConnector {
    async fn prompt(&self, context: &ExecutionContext) {
        let provider_id = context.executor_config.provider_id
            .as_deref()
            .unwrap_or(&self.default_provider);
        let model_id = context.executor_config.model_id
            .as_deref()
            .unwrap_or(&self.model_id);
        // 动态创建 bridge
    }
}
```

### 前端 Settings key 命名规范

```
llm.anthropic.api_key          (现有)
llm.openai.api_key             (现有)
llm.openai.base_url            (现有)
llm.openai.default_model       (现有)
llm.gemini.api_key             (新增)
llm.deepseek.api_key           (新增)
llm.groq.api_key               (新增)
llm.xai.api_key                (新增)
llm.ollama.base_url            (新增，无 api_key)
llm.openrouter.api_key         (新增)
```

### 涉及文件

**后端**：
- `crates/agentdash-executor/src/connector.rs`（ModelInfo 结构体）
- `crates/agentdash-executor/src/connectors/pi_agent.rs`（主要改动）
- `Cargo.toml`（确认 rig-core 版本 + 必要时 patch）

**前端**：
- `frontend/src/pages/SettingsPage.tsx`
- `frontend/src/features/executor-selector/model/types.ts`
- `frontend/src/features/executor-selector/ui/ExecutorSelector.tsx`
- `frontend/src/features/project/agent-preset-editor.tsx`
- `frontend/src/services/executor.ts`
- `frontend/src/types/index.ts`
