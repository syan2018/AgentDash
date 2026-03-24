# 前端 Agent 选择与会话集成

## 1. 背景与目标

后端已完成 `CompositeConnector` + `PiAgentConnector` + `VibeKanbanExecutorsConnector` 的架构。`/api/agents/discovery` 能返回所有可用执行器（包括 Pi Agent），`/api/sessions/{id}/prompt` 通过 `AgentDashExecutorConfig.executor` 路由到对应 connector。

但前端目前：
- `ExecutorSelector` 只显示 vibe-kanban 的执行器（因为 Pi Agent 之前不存在）
- 会话页面的消息渲染假设了 vibe-kanban ACP 消息格式
- 执行器配置仅存 localStorage，不与服务端 settings 同步
- Task Agent Binding 的 agent_type 选项来自 project config 的 presets

本任务的目标是：
1. 让 `ExecutorSelector` 正确展示 Pi Agent 并支持选择
2. 会话页面适配 Pi Agent 的消息流格式
3. Task Agent Binding 支持选择 Pi Agent
4. 执行器默认值从服务端 settings 读取（与 settings-config-panel 任务协同）

## 2. 当前约束

1. `useExecutorDiscovery` 调用 `GET /api/agents/discovery`，后端 `CompositeConnector.list_executors()` 已聚合所有连接器的执行器——Pi Agent 应已在列表中。
2. `useExecutorDiscoveredOptions` 通过 NDJSON stream 获取模型/变体选项——Pi Agent 的 `discover_options_stream` 目前返回空（PiAgentConnector 尚未实现此方法）。
3. `SessionPage` 中的消息通过 `useSessionMessages` + `AcpMessageBubble` 渲染——需确认 Pi Agent 的 ACP 消息格式兼容。
4. `useExecutorConfig` 持久化到 localStorage。

## 3. Goals / Non-Goals

### Goals

- **G1**: ExecutorSelector 展示 Pi Agent，标识为"内置"（区分于 vibe-kanban 的外部执行器）
- **G2**: Pi Agent 的 discovered options：模型选择（目前 Anthropic Sonnet / OpenAI gpt-5.4）
- **G3**: 会话页面正确渲染 Pi Agent 的回复消息和工具调用
- **G4**: Task Agent Binding 下拉列表包含 Pi Agent
- **G5**: 新建会话时可选 Pi Agent，会话能端到端完成（发送 prompt → 收到回复 → 显示）

### Non-Goals

- 不做 Pi Agent 的高级配置 UI（system prompt 等在 Settings 页面）
- 不实现多模型并行对比（后续功能）
- 不做执行器健康检查 UI（后续功能）

## 4. ADR-lite（核心决策）

### 决策 A：Pi Agent 在 ExecutorSelector 中的展示

Pi Agent 在 `list_executors()` 中返回时，带有 `connector_type: "pi-agent"`。
前端根据此字段给予不同的视觉标识：
- vibe-kanban 执行器：显示进程图标（外部）
- pi-agent：显示内置/星号图标，名称为 "AgentDash 内置 Agent"

### 决策 B：PiAgentConnector 实现 discover_options_stream

返回 Pi Agent 支持的模型列表（基于 settings 中已配置的 LLM Provider）：
- Anthropic 已配置 → `[claude-4-sonnet, claude-4-opus, ...]`
- OpenAI 已配置 → `[gpt-5.4, gpt-5.4-mini, ...]`
- 两者都配置 → 合并，按 provider 分组

### 决策 C：ACP 消息格式复用

Pi Agent 通过 PiAgentConnector 输出标准 ACP `SessionNotification` 消息。
前端已有的 `AcpMessageBubble` 等组件应能直接渲染——重点验证而非重写。

### 决策 D：执行器默认值同步策略

- `useExecutorConfig` 的 localStorage 值作为会话级覆盖
- 首次加载时从 `GET /api/settings?category=executor.default` 获取服务端默认值
- 用户手动选择后覆盖服务端默认值（localStorage 优先）
- "重置为默认" 按钮清除 localStorage，回落到服务端值

## 5. Signatures

### 5.1 PiAgentConnector discover_options_stream 返回

```rust
// agentdash-executor/src/connectors/pi_agent.rs
// discover_options_stream 返回的 NDJSON 条目

// 第一条：模型选择器
{
    "type": "model_selector",
    "data": {
        "providers": [
            {
                "id": "anthropic",
                "name": "Anthropic",
                "models": [
                    { "id": "claude-4-sonnet", "name": "Claude 4 Sonnet", "default": true },
                    { "id": "claude-4-opus", "name": "Claude 4 Opus" }
                ]
            },
            {
                "id": "openai",
                "name": "OpenAI",
                "models": [
                    { "id": "gpt-5.4", "name": "gpt-5.4", "default": true },
                    { "id": "gpt-5.4-mini", "name": "gpt-5.4 Mini" }
                ]
            }
        ]
    }
}
```

### 5.2 前端组件改动

```ts
// features/executor-selector/ui/ExecutorSelector.tsx
// 新增：Pi Agent 识别与特殊展示
interface ExecutorItemProps {
  executor: DiscoveredExecutor;
  isBuiltIn: boolean;  // NEW: 标识内置执行器
}

// features/executor-selector/model/useExecutorConfig.ts
// 新增：服务端默认值同步
interface UseExecutorConfig {
  // 现有...
  syncFromServerDefaults: () => Promise<void>;
  resetToDefaults: () => void;
}
```

### 5.3 Task Agent Binding 改动

```ts
// features/task/ui/agent-binding-fields.tsx
// agent_type 选项来源扩展：
// 1. project config 的 agent_presets (现有)
// 2. discovery 中的所有 executors (包括 Pi Agent)
```

## 6. Contracts

### 6.1 Pi Agent 执行器信息契约

```json
{
  "id": "pi-agent",
  "name": "AgentDash 内置 Agent",
  "connector_type": "pi-agent",
  "available": true,
  "variants": [],
  "capabilities": ["chat", "tool_use"]
}
```

### 6.2 Pi Agent ACP 消息格式

Pi Agent 通过 PiAgentConnector 发出的 ACP 消息应与 vibe-kanban 格式兼容：
- `SessionUpdate::Message { role: "assistant", content: [...] }`
- `SessionUpdate::ToolCall { id, name, input }`
- `SessionUpdate::ToolResult { tool_call_id, output }`
- `SessionUpdate::Finished { ... }`

前端渲染组件已支持上述消息类型。

### 6.3 执行器默认值同步契约

```
页面加载 → useExecutorConfig.init()
  → 检查 localStorage 
  → if 无值 → GET /api/settings?category=executor.default
  → 用服务端值作为初始值
  → 后续用户选择 → 写 localStorage

用户点击"重置为默认"
  → 清空 localStorage
  → 重新从服务端获取
```

## 7. 验收标准

- [ ] `ExecutorSelector` 中显示 Pi Agent，带"内置"标识
- [ ] 选择 Pi Agent 后，模型列表正确显示（基于已配置的 Provider）
- [ ] 新建会话选择 Pi Agent → 发送 prompt → 正确显示回复
- [ ] Pi Agent 工具调用在会话消息中正确渲染
- [ ] Task Agent Binding 下拉包含 Pi Agent 选项
- [ ] 执行器默认值首次从服务端 settings 获取
- [ ] "重置为默认" 功能正常

## 8. 依赖

- **settings-config-panel** (03-06)：提供服务端 settings API 和 LLM Provider 配置
- **agent-tool-system** (03-06)：Pi Agent 需要工具才能有实际意义的会话
- **已完成**：CompositeConnector、PiAgentConnector、AgentDashExecutorConfig

## 9. 实施拆分（建议）

### Phase 1: 后端适配（约 1.5h）
1. PiAgentConnector 实现 `discover_options_stream`（返回模型列表）
2. PiAgentConnector 的 executor info 带 `connector_type` 标识

### Phase 2: ExecutorSelector 改进（约 2h）
3. 识别并特殊展示内置执行器
4. Pi Agent 模型选择支持
5. 服务端默认值同步

### Phase 3: 会话集成验证（约 1.5h）
6. 创建 Pi Agent 会话端到端测试
7. 消息渲染兼容性验证
8. 工具调用显示验证

### Phase 4: Task Agent Binding（约 1h）
9. 扩展 agent_type 选项来源
10. Pi Agent 选择后的配置传递
