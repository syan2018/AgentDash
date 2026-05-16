# LLM 模型配置跨层契约

> 模型选择、推理级别、provider 管理的跨层约定。

---

## 核心参数

Agent 运转只需要三个业务参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `model_id` | `String` | 使用的模型 ID |
| `provider_id` | `String` | 所属 provider（如 `anthropic`、`gemini`） |
| `thinking_level` | `ThinkingLevel` | 推理强度枚举 |

`temperature`、`max_tokens`、`top_p` 等不暴露给业务层。

---

## ThinkingLevel 枚举

后端定义在 `agentdash-domain/src/common/agent_config.rs`，前端定义在 `types/index.ts`。

序列化值（前后端一致）：`"off"` / `"minimal"` / `"low"` / `"medium"` / `"high"` / `"xhigh"`

**关键约束**：`ModelInfo.reasoning == true` 的模型才在 UI 中显示 ThinkingLevel 选择器。

---

## AgentConfig 关键字段

定义在 `agentdash-domain/src/common/agent_config.rs`：
`executor`、`provider_id`、`model_id`、`agent_id`、`thinking_level`、`permission_policy`、`system_prompt`、`system_prompt_mode`

注意：旧 `tool_clusters` 已删除，由 `AgentPresetConfig.capability_directives` 替代。

---

## ModelInfo 前端契约

定义在 `features/executor-selector/model/types.ts`，关键字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `string` | 模型 ID |
| `name` | `string` | 显示名称 |
| `provider_id` | `string?` | 所属 provider |
| `reasoning` | `boolean` | 是否支持 extended thinking |
| `supports_image` | `boolean` | 是否支持图像输入 |
| `context_window` | `number` | 上下文窗口大小 |
| `blocked` | `boolean?` | 是否被 provider 设置屏蔽 |
| `discovered` | `boolean?` | 是否来自 API 动态发现 |

---

## PiAgent Provider Registry

### 已支持的 Provider

| provider_id | 服务商 | API Key 设置 key | 备注 |
|-------------|--------|-----------------|------|
| `anthropic` | Anthropic Claude | `llm.anthropic.api_key` | 优先级最高 |
| `gemini` | Google Gemini | `llm.gemini.api_key` | 支持 thinking |
| `deepseek` | DeepSeek | `llm.deepseek.api_key` | R1 支持 reasoning |
| `groq` | Groq | `llm.groq.api_key` | 高速推理 |
| `xai` | xAI (Grok) | `llm.xai.api_key` | Grok Mini 支持 thinking |
| `openai` | OpenAI / 兼容端点 | `llm.openai.api_key` | 支持自定义 `base_url` 与 `wire_api` |

注册优先级（第一个为默认）：`anthropic → gemini → deepseek → groq → xai → openai`

### OpenAI `wire_api` 契约

`llm.openai.wire_api` 控制运行时 bridge 选择：

| 值 | 作用 |
|----|------|
| `responses` | 使用 OpenAI Responses API 路径（官方端点默认） |
| `completions` | 使用 Chat Completions 路径（自定义兼容端点默认） |

默认策略：`base_url` 为空或为官方地址时用 `responses`，自定义端点时用 `completions`。原因是自定义兼容端点的 tool-call delta 流更接近 Chat Completions 语义。

---

## Settings Key 命名

```
llm.anthropic.api_key
llm.gemini.api_key
llm.deepseek.api_key
llm.groq.api_key
llm.xai.api_key
llm.openai.api_key
llm.openai.base_url          # 可选，OpenAI 兼容端点
llm.openai.default_model     # 可选，默认模型 ID
llm.openai.wire_api          # 可选：responses / completions
llm.ollama.base_url          # 本地 Ollama，无 api_key
agent.pi.system_prompt       # PiAgent 系统提示词
```

---

## 数据流

```
前端 ExecutorSelector → { executor, model_id, thinking_level }
  → POST /sessions/{id}/prompt
  → 后端按 model_id 选择 provider bridge
  → agent.set_thinking_level(thinking_level)
  → AgentLoop → LLM API
```

---

## 禁止模式

- 在 AgentConfig 中暴露 `temperature` / `max_tokens` 给外部配置
- 用 `reasoning_id: String` 表示推理强度（改用 `ThinkingLevel` 枚举）
- 构建时硬编码 model 而非运行时按 `model_id` 选择
