# 清理模型底层参数：统一 model + thinkingLevel 契约

## 背景

当前代码中存在大量从 LLM 底层 API 直接抄入的参数（`temperature`、`top_p`、`max_tokens`、`budget_tokens`），这些参数在 Agent 业务层是无意义的噪声：
- `temperature`：在 coding agent 场景下固定接近 0，无需用户感知
- `top_p`/`top_k`：同上，底层采样参数，业务层不关心
- `max_tokens`：由 `Model.maxTokens`（模型元数据）决定，不应由用户在运行时配置
- `budget_tokens`：已被 `thinkingBudgets`（按 level 固定预算）替代

参考 `references/pi-mono`，Agent 运转真正需要的参数层次：
- **核心语义**：`provider`、`model_id`、`thinking_level`
- **行为控制**：`thinking_budgets`（按 level 的 token 上限，配置层），`tool_execution`（`sequential`/`parallel`）
- **传输元数据**：`cache_retention`（由系统决定，不暴露给用户）

## Goal

1. 删除 `AgentConfig`、`BridgeRequest`、`AgentDashExecutorConfig` 中无用的底层参数（`temperature`、`max_tokens`、`top_p`）
2. 将 `ThinkingLevel` 正式纳入 `AgentDashExecutorConfig` 和 `AgentBinding`（替代不明确的 `reasoning_id: Option<String>`）
3. 打通 PiAgent 链路：`AgentDashExecutorConfig.thinking_level` → `AgentConfig.thinking_level` → `BridgeRequest` → LLM 调用
4. 删除 Settings 页中的 `agent.pi.temperature` / `agent.pi.max_turns` 全局配置（前端 + 后端）
5. 前端 ExecutorSelector 中将 `reasoningId`（自由字符串）替换为类型化的 `ThinkingLevel` 枚举选择器

## Requirements

### 后端

- [ ] **删除 `AgentConfig.temperature`、`AgentConfig.max_turns`（作为 Settings key）**
  - 文件：`crates/agentdash-agent/src/agent.rs`
  - `temperature` 字段直接删除；`max_turns` 保留为代码常量（默认 25），不对外暴露

- [ ] **删除 `BridgeRequest.temperature`、`BridgeRequest.max_tokens`**
  - 文件：`crates/agentdash-agent/src/bridge.rs`
  - `max_tokens` 从 `Model.maxTokens` 读取（通过模型元数据），不从外部传入
  - Rig 调用时用模型自身的默认值

- [ ] **`AgentDashExecutorConfig` 将 `reasoning_id: Option<String>` 改为 `thinking_level: Option<ThinkingLevel>`**
  - 文件：`crates/agentdash-executor/src/connector.rs`
  - `ThinkingLevel` 枚举：`Off | Minimal | Low | Medium | High | Xhigh`
  - 保持 VibeKanban 路径的 `reasoning_id` 转换（将 `ThinkingLevel` 序列化为 vibe-kanban 期望的字符串）

- [ ] **打通 PiAgent thinking_level 注入路径**
  - 文件：`crates/agentdash-executor/src/connectors/pi_agent.rs`
  - `prompt()` 中从 `context.executor_config.thinking_level` 读取并填充 `AgentConfig.thinking_level`
  - `AgentConfig.thinking_level` → `AgentLoopConfig` → `BridgeRequest` 中的 extended thinking 参数

- [ ] **打通 PiAgent model_id 注入路径**（顺带修复，属于本 task 范围内）
  - 文件：`crates/agentdash-executor/src/connectors/pi_agent.rs`
  - `prompt()` 中用 `context.executor_config.model_id` 动态选择模型（而非构建时固定）
  - 意味着 `PiAgentConnector` 需要持有 client 而非固定 model，运行时动态 `.completion_model(id)`

- [ ] **删除 Settings Key：`agent.pi.temperature`、`agent.pi.max_turns`**
  - 找出 settings 初始化/读取代码，删除这两个 key 的读写路径
  - 数据库中的存量 key 通过 migration 或启动时 cleanup 清除

- [ ] **`AgentBinding` 增加 `thinking_level: Option<ThinkingLevel>` 字段**
  - 文件：`crates/agentdash-domain/src/task/value_objects.rs`
  - `CreateTaskAgentBindingRequest` 同步增加该字段（`crates/agentdash-api/src/routes/stories.rs`）

### 前端

- [ ] **删除 Settings 页中 `temperature`/`max_turns` 表单项**
  - 文件：`frontend/src/pages/SettingsPage.tsx`
  - 删除对应的 `Field` 组件和 `SettingUpdate` 调用

- [ ] **`ExecutorSelector` 将 `reasoningId` 下拉改为 `ThinkingLevel` 枚举**
  - 文件：`frontend/src/features/executor-selector/ui/ExecutorSelector.tsx`
  - `frontend/src/features/executor-selector/model/types.ts`
  - `frontend/src/services/executor.ts`
  - 将 `reasoning_id: string` 改为 `thinking_level: ThinkingLevel | undefined`
  - 枚举项：`off | minimal | low | medium | high | xhigh`
  - 展示条件：当所选模型 `model.reasoning === true` 时才显示（从 `ModelInfo.reasoning` 字段判断）

- [ ] **`AgentBindingFields` 增加 `thinking_level` 选择器**
  - 文件：`frontend/src/features/task/agent-binding-fields.tsx`

- [ ] **类型定义同步更新**
  - `frontend/src/types/index.ts`：`AgentBinding` 增加 `thinking_level` 字段，`ExecutorConfig` 中 `reasoning_id` 改为 `thinking_level`
  - `frontend/src/features/executor-selector/model/types.ts`：`PersistedExecutorConfig` 同步更新

- [ ] **`useExecutorConfig` localStorage 迁移**
  - 版本化 localStorage key 或清除旧 `reasoningId` 字段，避免读到残留数据

## Acceptance Criteria

- [ ] 代码库中不再出现 `temperature`（作为 Agent/Bridge 配置字段）、`top_p`、`top_k`、`budget_tokens`（字面量）
- [ ] `reasoning_id: Option<String>` 在 `AgentDashExecutorConfig` 中已被 `thinking_level: Option<ThinkingLevel>` 替换
- [ ] Settings 页面没有 temperature/max_turns 输入框
- [ ] ExecutorSelector 的推理模式下拉是枚举选项，不是自由文本
- [ ] PiAgent 的 thinking_level 实际影响 LLM 调用（可通过日志或 network 验证）
- [ ] PiAgent 的 model_id 运行时 override 生效（选不同模型发出不同 model 字段的请求）
- [ ] 现有 VibeKanban 路径行为不变（VibeKanban 通过 ThinkingLevel → reasoning_id 字符串转换兼容）

## Technical Notes

### ThinkingLevel 枚举对照

```
ThinkingLevel (Rust) ←→ reasoning_options[].id (前端/VibeKanban)
Off      → "off"
Minimal  → "minimal"
Low      → "low"
Medium   → "medium"
High     → "high"
Xhigh    → "xhigh"
```

VibeKanban 路径的兼容转换：`AgentDashExecutorConfig.thinking_level.to_reasoning_id_str()` → 传给 vibe-kanban CLI 的 `--reasoning` 参数。

### PiAgent 动态模型选择

当前 `build_pi_agent_connector` 构建时固定了模型，需要重构为：
- 构建时只创建 `Client`（Anthropic / OpenAI），不绑定具体模型
- `prompt()` 调用时从 `context.executor_config.model_id` 动态调用 `client.completion_model(id)`
- 回退逻辑：`model_id` 为空时用 settings 中的 `llm.anthropic.default_model` / `llm.openai.default_model`

### 涉及文件

**后端**：
- `crates/agentdash-agent/src/agent.rs`
- `crates/agentdash-agent/src/bridge.rs`
- `crates/agentdash-agent/src/types.rs`（ThinkingLevel 枚举已存在，确认枚举值对齐）
- `crates/agentdash-executor/src/connector.rs`
- `crates/agentdash-executor/src/connectors/pi_agent.rs`
- `crates/agentdash-executor/src/connectors/vibe_kanban.rs`
- `crates/agentdash-domain/src/task/value_objects.rs`
- `crates/agentdash-api/src/routes/stories.rs`

**前端**：
- `frontend/src/pages/SettingsPage.tsx`
- `frontend/src/features/executor-selector/ui/ExecutorSelector.tsx`
- `frontend/src/features/executor-selector/model/types.ts`
- `frontend/src/features/executor-selector/model/useExecutorConfig.ts`
- `frontend/src/features/task/agent-binding-fields.tsx`
- `frontend/src/services/executor.ts`
- `frontend/src/types/index.ts`
