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
`executor`、`provider_id`、`model_id`、`agent_id`、`thinking_level`、`system_prompt`、`system_prompt_mode`

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

用户 BYOK 凭据同时保存验证状态：`verification_status` 表示 `unverified` / `verified` / `failed`，`verification_message` 保存最近一次非敏感探测结果，`verified_at` 保存最近一次通过验证的时间。手动 API Key 保存会通过 Provider 的模型探测路径更新验证状态；Codex OAuth token exchange 成功即视为 OAuth 凭据已验证。

`credential_mode` 控制运行态凭据解析：

| 值 | 运行态含义 |
|----|------------|
| `global_only` | 只使用管理员全局 DB Key 或 `env_api_key` |
| `global_or_user` | 当前用户有 BYOK 时优先使用用户 Key，否则使用全局 Key |
| `user_required` | 只使用当前用户 BYOK；没有用户身份或用户 Key 时不可执行 |

Native Runtime provisioning按`AuthIdentity`构造明确credential scope；factory通过`RepositoryNativeBridgeResolver`解析effective Provider。无身份只允许显式platform scope，user_required不回退。OpenAI-compatible本地无Key端点可声明`credential_source=none`。

DB-backed Key 的加解密通过 `LlmSecretCodec` 端口完成，当前基础设施实现使用 AES-GCM 主密钥：部署可以通过 `AGENTDASH_SECRET_KEY` 显式指定；未指定时服务端会在 AgentDash 数据根下创建并复用本地 master key 文件。这样本地开发和 embedded Postgres 数据生命周期保持一致，同时 API 响应只暴露配置状态、来源和脱敏 preview，运行态 secret 只在 resolver 到 bridge 构建链路内短暂存在。

`openai_codex` 的凭据内容是 ChatGPT OAuth token JSON，不是用户可直接获取的 API Key。管理员全局 Codex 登录写入 Provider 的 `global_api_key_ciphertext`，用户个人 Codex 登录写入 `llm_provider_user_credentials`；两者复用同一套 PKCE 回调与 token exchange，只在保存目标上区分所有权。API 响应对 Codex 凭据只展示 OAuth 状态 preview，不返回 token JSON 或其中任意字段。

桌面 OAuth bridge 的平台 access token 是可选传输字段：Personal 模式没有 Bearer 时由 API AuthProvider 建立固定 local identity；Enterprise 模式缺少有效 token 在统一认证中返回 401，已认证但非管理员访问全局 Provider 管理入口返回 403。OAuth flow 以发起 identity 作为短时 owner 防止劫持，但 global credential 的持久化 ownership 仍属于 Provider catalog，不随 flow owner 转为用户 BYOK。

### Scenario: Desktop ChatGPT OAuth 授权边界

#### 1. Scope / Trigger

- Trigger：桌面端为 `openai_codex` Provider 发起 PKCE OAuth，目标为 `global_provider` 或 `user_byok`。

#### 2. Signatures

```text
POST /llm-providers/{id}/codex-oauth/desktop/prepare
POST /llm-providers/{id}/user-credential/codex-oauth/desktop/prepare
POST /llm-providers/codex-oauth/{flow_id}/{complete|fail|cancel}

DesktopCodexOAuthStartRequest {
  api_origin: string,
  access_token?: string,
  provider_id: string,
  target: global_provider | user_byok,
}
```

#### 3. Contracts

- Desktop bridge 仅在 `access_token` 存在且非空时发送 Bearer；缺失时省略 Authorization。
- API AuthProvider 是身份事实源。Personal 模式建立 local identity；Enterprise 模式验证真实登录 token。
- flow owner 仅约束 status/cancel/complete/fail 的短时访问；credential target 决定最终保存到 Provider global ciphertext 或 user credential repository。
- 全局 Provider 的运行态 credential 不随发起 OAuth 的用户变化。

#### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Personal + 无平台 token | prepare 成功，flow owner 为 local identity |
| Enterprise + 无效或缺失 token | 统一认证返回 401 |
| Enterprise + 已认证非管理员 + global target | `require_system_access` 返回 403 |
| Enterprise admin + global target | prepare 成功，完成后写 `global_api_key_ciphertext` |
| 任意身份完成他人 flow | owner 校验拒绝，不交换或保存 credential |
| user_byok target | 保存到 `provider_id + user_id` 唯一用户凭据 |

#### 5. Good / Base / Bad Cases

- Good：Personal 桌面没有平台登录 token，仍可完成全局 ChatGPT OAuth；凭据成为平台 Provider 配置。
- Base：Enterprise 桌面透传当前平台 token，由 API 判断管理员权限。
- Bad：Web 或 Tauri 在请求到达 API 前以“当前会话未登录”拒绝，因为它绕过了 AuthProvider 的模式语义。

#### 6. Tests Required

- Web：无 stored token 仍调用 desktop bridge；有 token 时正确透传。
- Tauri：`None` 不产生 Authorization，`Some` 产生 Bearer。
- API：Personal allow、Enterprise non-admin 403、admin allow；flow owner 与 credential target 分离。
- Product path：无 Bearer 创建临时全局 Codex Provider并调用 desktop prepare，断言返回 flow ID/auth URL。

#### 7. Wrong vs Correct

```ts
// Wrong
if (!getStoredToken()) throw new Error("需要当前登录会话")

// Correct
const token = getStoredToken()
desktop.startCodexOAuth({ ...request, ...(token ? { access_token: token } : {}) })
```

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
AgentRun product command + AuthIdentity
  → Business Surface编译provider/model期望
  → Native service instance(config + credential scope)
  → trusted factory解析LlmBridge
  → Host offer/binding + Managed Runtime operation
  → Native Driver → AgentLoop → LLM API
```

## Scenario: Companion SubAgent Model Preflight

### 1. Scope / Trigger

- Trigger: Companion `target=sub` dispatch 会创建 child AgentRun、mailbox intake 和可选 durable gate wait policy。
- Scope: `CompanionRequestTool`、`CollaborationRuntimeToolProvider`、API session bootstrap、PiAgent provider registry effective profile catalog、LLM Provider credential resolver。

### 2. Signatures

Application preflight port:

```rust
pub struct CompanionModelPreflightRequest {
    pub project_id: Uuid,
    pub parent_run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub selected_project_agent_id: Uuid,
    pub selected_agent_key: String,
    pub companion_label: String,
    pub executor_config: AgentConfig,
    pub identity: Option<AuthIdentity>,
}

#[async_trait]
pub trait CompanionModelPreflightPort: Send + Sync {
    async fn preflight_companion_model(
        &self,
        request: CompanionModelPreflightRequest,
    ) -> Result<(), CompanionModelPreflightError>;
}
```

Effective catalog helper:

```rust
build_effective_profile_catalog_from_db(
    llm_provider_repo,
    llm_provider_credential_repo,
    llm_secret_codec,
    identity,
)
```

### 3. Contracts

- SubAgent dispatch runs model preflight after resolving the selected ProjectAgent and before hook dispatch planning, `LifecycleDispatchService::open_interaction_gate`, `launch_agent`, child mailbox intake, or gate wait policy creation.
- The preflight fact source is the provider/account effective profile catalog: provider enabled state, credential mode, global/user credential resolution, dynamic model discovery when supported, configured models, default model and blocked models.
- The preflight request carries the current `AuthIdentity` so `global_or_user` and `user_required` providers resolve with the same account context as runtime execution.
- API bootstrap owns the concrete preflight adapter because it has both `RepositorySet` and `LlmSecretCodec`; application companion code only consumes `CompanionModelPreflightPort`.
- `openai_codex` credentials are account-scoped ChatGPT OAuth credentials. When no stable account model discovery exists, preflight validates the effective provider and configured/blocked model contract; runtime provider 400 remains a terminal fact that gate producer terminal convergence must resolve.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| `provider_id` and `model_id` are both missing | Return visible config error before dispatch side effects. |
| `provider_id` does not match an effective provider | Return visible provider config error. |
| Provider is disabled or lacks effective credential for current identity | Return visible provider/account credential error. |
| Explicit provider is executable but selected/default model is missing | Return visible model config error. |
| Selected model is blocked | Return visible blocked-model error. |
| Only `model_id` is supplied and multiple executable providers expose it | Return visible ambiguity error requiring `provider_id`. |
| Effective provider/model passes preflight | Continue normal companion dispatch. |
| Runtime provider later returns terminal 400 despite preflight | Runtime terminal convergence remains responsible for resolving any existing gate wait policy. |

### 5. Good/Base/Bad Cases

- Good: A parent Agent dispatches a SubAgent whose selected ProjectAgent uses `provider_id=openai` and `model_id=gpt-5`; the effective profile confirms the current account can execute the model, then dispatch opens any required gate.
- Base: A configured `openai_codex` model passes configured/blocked model preflight because no stable model discovery endpoint exists; execution still carries the ChatGPT account credential and terminal convergence handles runtime-only account limits.
- Bad: Dispatch creates a durable wait gate before checking provider executable state, because the caller may then observe a long-lived pending wait for a configuration error that was knowable before dispatch.

### 6. Tests Required

- Unit: effective model preflight accepts explicit provider/model.
- Unit: effective model preflight rejects blocked model.
- Unit: effective model preflight rejects unavailable provider for the current credential mode/identity.
- Unit: effective model preflight rejects ambiguous model selection when provider is omitted.
- Application/API check: companion runtime tool provider compiles with injected preflight port and `CompanionRequestTool` calls it before dispatch.

### 7. Wrong vs Correct

#### Wrong

```text
companion_request(target=sub)
  -> open wait gate
  -> create child AgentRun/mailbox
  -> runtime discovers provider/model is not executable
```

#### Correct

```text
companion_request(target=sub)
  -> resolve selected ProjectAgent executor_config
  -> provider/account effective model preflight
  -> open wait gate / launch child only when executable
```

## Native Agent Runtime 模型绑定

Native Integration service instance保存明确provider/model与credential scope；factory激活时通过repository+secret codec解析`LlmBridge`。配置与surface revision绑定到durable Runtime binding，不能在turn中静默切换provider。

### 契约

- user credential scope必须带authenticated user coordinate，不回退platform credential。
- provider/model不存在、blocked或credential缺失在Driver factory side effect前返回typed unavailable/invalid configuration。
- model/surface变化创建新的service instance revision/offer generation；已有binding保持sticky，显式rebind才采用新revision。
- secret不进入instance JSON、RuntimeOffer、RuntimeWire或日志。
- 测试覆盖provider extraction、scope、error mapping与无fallback。

---

## 禁止模式

- 在 AgentConfig 中暴露 `temperature` / `max_tokens` 给外部配置
- 用 `reasoning_id: String` 表示推理强度（改用 `ThinkingLevel` 枚举）
- 构建时硬编码 model 而非运行时按 `model_id` 选择
