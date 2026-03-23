# 修复 PiAgent 硬编码模型列表问题

## 问题描述

PiAgent Connector 当前硬编码了各 LLM Provider 的模型列表：

- `OPENAI_MODELS` - 硬编码 gpt-4o, o3, o4-mini
- `ANTHROPIC_MODELS` - 硬编码 claude-opus-4-6, claude-sonnet-4-6 等
- `GEMINI_MODELS`、`DEEPSEEK_MODELS`、`GROQ_MODELS`、`XAI_MODELS`

这导致以下问题：
1. 无法使用自定义 OpenAI 兼容端点的真实可用模型
2. 无法使用新发布的模型（必须改代码重新编译）
3. 无法限制只使用特定模型子集
4. 模型元数据（context_window、reasoning 等）可能与实际不符

## 目标

1. **清理硬编码** - 移除所有 `const XXX_MODELS` 静态数组
2. **动态获取优先** - 调用各 Provider 的 list models API 获取真实可用模型
3. **配置兜底** - 获取不到时支持用户通过配置声明可用模型
4. **前端适配** - 配置界面支持动态模型输入/选择

## 技术方案

### 后端改造

#### 1. 移除硬编码（pi_agent.rs）

删除以下静态数组：
```rust
// 删除这些
const ANTHROPIC_MODELS: &[KnownModelMeta] = & [...];
const GEMINI_MODELS: &[KnownModelMeta] = & [...];
const DEEPSEEK_MODELS: &[KnownModelMeta] = & [...];
const GROQ_MODELS: &[KnownModelMeta] = & [...];
const XAI_MODELS: &[KnownModelMeta] = & [...];
const OPENAI_MODELS: &[KnownModelMeta] = & [...];
```

#### 2. 添加 list_models 能力

为每个 Provider 客户端添加异步获取模型列表的方法：

```rust
#[async_trait]
trait ListModels {
    async fn list_models(&self) -> Result<Vec<ModelInfo>, Box<dyn Error>>;
}
```

- OpenAI: `GET /v1/models`
- Anthropic: 暂无官方 list models API，使用配置或默认值
- Gemini: `GET /v1beta/models`
- DeepSeek: `GET /v1/models`
- Groq: `GET /v1/models`
- xAI: `GET /v1/models`

#### 3. 配置扩展

支持用户自定义模型列表：

```json
{
  "llm.openai.api_key": "sk-xxx",
  "llm.openai.base_url": "https://api.openai.com/v1",
  "llm.openai.models": [
    {"id": "gpt-4o", "name": "GPT-4o", "context_window": 128000, "supports_reasoning": false},
    {"id": "gpt-4o-mini", "name": "GPT-4o Mini", "context_window": 128000, "supports_reasoning": false}
  ]
}
```

#### 4. 发现逻辑改造

`discover_options_stream` 的优先级：

1. 调用 Provider `list_models()` API 获取
2. 若失败，读取用户配置的 `llm.{provider}.models`
3. 若未配置，使用最小默认集合（只包含默认模型）
4. 允许手动输入任意模型 ID（前端支持）

### 前端改造

#### 1. 模型选择器增强

- 支持从 discovered options 获取模型列表
- 支持下拉选择 + 手动输入混合模式
- 显示模型元数据（context window、是否支持 reasoning）

#### 2. 配置界面

- Provider 配置表单添加模型列表编辑器
- 支持添加/删除/编辑模型
- 支持从 Provider API 一键同步模型列表

## 验收标准

- [ ] PiAgent 中无硬编码模型列表
- [ ] OpenAI Provider 支持调用 /v1/models 获取模型
- [ ] 支持通过配置 `llm.{provider}.models` 自定义模型
- [ ] 前端模型选择器支持手动输入模型 ID
- [ ] 配置界面支持模型列表管理
- [ ] 向后兼容：现有配置仍能正常工作

## 影响范围

- `crates/agentdash-executor/src/connectors/pi_agent.rs`
- `crates/agentdash-domain/src/settings/`（可能需要）
- 前端配置相关组件

## 风险

- **API 兼容性**：各 Provider 的 list models API 格式不同，需单独适配
- **性能**：首次发现模型时需要网络请求，可能延迟 1-3 秒
- **向后兼容**：需要确保现有硬编码配置仍能工作

## 相关代码

- 硬编码位置：`crates/agentdash-executor/src/connectors/pi_agent.rs:48-162`
- Provider 注册位置：`crates/agentdash-executor/src/connectors/pi_agent.rs:1167-1359`
- 发现选项实现：`crates/agentdash-executor/src/connectors/pi_agent.rs:467-520`
