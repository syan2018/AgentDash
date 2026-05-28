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

能力配置由 `AgentPresetConfig.capability_directives` 表达。

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

### Provider Baseline

| provider_id | 服务商 | API Key 设置 key | 备注 |
|-------------|--------|-----------------|------|
| `anthropic` | Anthropic Claude | `llm.anthropic.api_key` | 优先级最高 |
| `gemini` | Google Gemini | `llm.gemini.api_key` | 支持 thinking |
| `deepseek` | DeepSeek | `llm.deepseek.api_key` | R1 支持 reasoning |
| `groq` | Groq | `llm.groq.api_key` | 高速推理 |
| `xai` | xAI (Grok) | `llm.xai.api_key` | Grok Mini 支持 thinking |
| `openai` | OpenAI / 兼容端点 | `llm.openai.api_key` | 支持自定义 `base_url` 与 `wire_api` |

注册优先级（第一个为默认）：`anthropic → gemini → deepseek → groq → xai → openai`

### Provider Catalog 与 BYOK 凭据

`llm_providers` 是管理员维护的全局 Provider Catalog：它保存 provider metadata、模型列表、屏蔽列表、端点、wire_api、排序、启用状态和 `credential_mode`。DB-backed 全局 Key 不进入领域实体明文字段，而是保存为 `global_api_key_ciphertext`；用户 BYOK Key 保存在 `llm_provider_user_credentials`，按 `provider_id + user_id` 唯一隔离。

`credential_mode` 控制运行态凭据解析：

| 值 | 运行态含义 |
|----|------------|
| `global_only` | 只使用管理员全局 DB Key 或 `env_api_key` |
| `global_or_user` | 当前用户有 BYOK 时优先使用用户 Key，否则使用全局 Key |
| `user_required` | 只使用当前用户 BYOK；没有用户身份或用户 Key 时不可执行 |

PiAgent prompt 和 discovered-options discovery 需要按当前 `AuthIdentity` 解析 effective Provider。HTTP prompt 已通过 `ExecutionSessionFrame.identity` 携带身份；discovered-options 通过 `DiscoveryContext.identity` 传入 connector。无身份的系统级执行只解析全局凭据。OpenAI-compatible 的本地无 Key 端点可以以 `credential_source = none` 进入运行态，原因是该协议承载 Ollama 等无鉴权端点；`user_required` 不适用无 Key 端点。

DB-backed Key 的加解密通过 `LlmSecretCodec` 端口完成，当前基础设施实现使用 AES-GCM 主密钥：部署可以通过 `AGENTDASH_SECRET_KEY` 显式指定；未指定时服务端会在 AgentDash 数据根下创建并复用本地 master key 文件。这样本地开发和 embedded Postgres 数据生命周期保持一致，同时 API 响应只暴露配置状态、来源和脱敏 preview，运行态 secret 只在 resolver 到 bridge 构建链路内短暂存在。

`openai_codex` 的凭据内容是 ChatGPT OAuth token JSON，不是用户可直接获取的 API Key。管理员全局 Codex 登录写入 Provider 的 `global_api_key_ciphertext`，用户个人 Codex 登录写入 `llm_provider_user_credentials`；两者复用同一套 PKCE 回调与 token exchange，只在保存目标上区分所有权。API 响应对 Codex 凭据只展示 OAuth 状态 preview，不返回 token JSON 或其中任意字段。

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

## PiAgent Live Runtime 模型切换

PiAgent 的 `LlmBridge` 在 `Agent::new(bridge, config)` 时绑定具体 provider/model。
同一个 `session_id` 的 live runtime 会跨 turn 复用 Agent 以保留会话历史、工具状态和
identity prompt，因此后续 prompt 的 `executor_config.provider_id/model_id` 变化必须在
connector 边界显式投影到 Agent bridge。

### 契约

- 每次 `PiAgentConnector::prompt` 从 `ExecutionContext.session.executor_config` 读取当前
  `provider_id` 与 `model_id`，空白值按 `None` 处理。
- live runtime 记录上一次已绑定 bridge 的模型选择。
- 如果当前模型选择与 live runtime 记录不同，connector 必须用新 bridge 重建 Agent，并把
  existing Agent 的消息历史、当前工具列表和已应用的 identity prompt 带入新 Agent。
- `thinking_level` 不要求重建 Agent；每轮 prompt 前调用 `agent.set_thinking_level(...)`
  即可生效。
- `SessionMeta.executor_config` 只记录会话生效配置；真正发往 LLM 的模型由 connector
  runtime bridge 决定，测试需要覆盖 connector bridge 选择而不只检查 meta。

---

## 禁止模式

- 在 AgentConfig 中暴露 `temperature` / `max_tokens` 给外部配置
- 用 `reasoning_id: String` 表示推理强度（改用 `ThinkingLevel` 枚举）
- 构建时硬编码 model 而非运行时按 `model_id` 选择
