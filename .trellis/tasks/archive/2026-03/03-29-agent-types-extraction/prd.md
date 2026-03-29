# Agent 核心类型抽取与 LLM Bridge 下沉重构

## 背景

经过完整的后端架构 Review，发现以下核心问题：

1. **`agentdash-spi` 定位模糊**：混合了纯数据类型（`AgentMessage`、`ContentPart`）、运行时 trait（`AgentRuntimeDelegate`）和 Hook 状态类型（`SessionHookSnapshot`），导致所有依赖 SPI 的 crate 被拖入 tokio/ACP 运行时
2. **`agentdash-agent` 无法独立复用**：Agent Loop 的核心概念（消息、工具、上下文）定义在 SPI 层而非 Agent 自身，且 rig-core LLM SDK 直接绑定在 Agent crate 中
3. **Hook 嵌入与 Agent 纯净性的矛盾**：Agent Loop 需要 hook 回调点，Hook 系统需要消费 Agent 的消息类型，双向需求导致"谁定义谁"的归属困境

## 目标

将 `agentdash-agent` 重构为一个**纯粹的 Agent Loop 引擎**：
- 它知道消息、工具、上下文、hook 回调点
- 它不知道具体用什么 LLM SDK 调用（通过 `LlmBridge` trait 注入）
- 它不知道 Hook 的具体规则（通过 `AgentRuntimeDelegate` trait 注入）

## 确定方案

### 设计决策记录

| 决策点 | 选项 | 结论 |
|--------|------|------|
| AgentTool trait 归属 | A:留SPI / B:搬agent / **C:放agent-types** | C — 放宽 agent-types 依赖约束，允许 async-trait + tokio-util |
| AgentRuntimeDelegate 注入方式 | A:纯回调 / B:搬agent / **C:放agent-types** | C — 与 AgentTool 保持一致性，delegate + DTO 全在 agent-types |
| AgentContext 作用域 | A:双Context / B:留agent / **C:ToolDefinition版入agent-types** | C — context.tools 改为 `Vec<ToolDefinition>`，loop 内部另持有工具查找表 |

### 目标依赖关系

```
agentdash-agent-types   (serde + async-trait + tokio-util，无 tokio runtime)
  ├─ 纯数据: ContentPart, AgentMessage, ToolCallInfo, StopReason, TokenUsage,
  │          AgentToolResult, AgentToolError, ToolDefinition, AgentContext
  ├─ Async traits: AgentTool, AgentRuntimeDelegate
  └─ Delegate DTOs: BeforeToolCallInput, AfterToolCallInput, 等全套 DTO
         ↑                ↑                ↑
  agentdash-agent    agentdash-spi    agentdash-executor
  (Loop + Bridge +   (Connector/Hook   (pi_agent/ 下:
   Builtins,          traits,            RigBridge + convert,
   无 rig/spi)        re-export types)   rig-core 仅此处)
```

### 新增 crate：`agentdash-agent-types`

依赖：serde, serde_json, thiserror, async-trait, tokio-util (CancellationToken), anyhow

| 模块 | 来源 | 内容 |
|------|------|------|
| `content.rs` | spi/tool.rs | `ContentPart` |
| `message.rs` | spi/lifecycle.rs | `AgentMessage`, `ToolCallInfo`, `StopReason`, `TokenUsage`, `now_millis()` |
| `tool.rs` | spi/tool.rs + 新增 | `AgentTool` trait, `AgentToolResult`, `AgentToolError`, `DynAgentTool`, `ToolUpdateCallback`, **`ToolDefinition`**(新增) |
| `context.rs` | spi/lifecycle.rs | `AgentContext`（tools 字段为 `Vec<ToolDefinition>`） |
| `delegate.rs` | spi/lifecycle.rs | `AgentRuntimeDelegate` trait, `DynAgentRuntimeDelegate`, `AgentRuntimeError` |
| `decisions.rs` | spi/lifecycle.rs | `ToolCallDecision`, `BeforeToolCallInput`, `AfterToolCallInput`, `AfterTurnInput`, `BeforeStopInput`, `TransformContextInput/Output`, `AfterToolCallEffects`, `TurnControlDecision`, `StopDecision` |
| `hooks_io.rs` | spi/lifecycle.rs | `BeforeToolCallResult`, `AfterToolCallResult`, `BeforeToolCallContext`, `AfterToolCallContext`, `ToolApprovalRequest`, `ToolApprovalOutcome` |

### 重构 `agentdash-spi`

- 移除 A 类纯数据类型（已迁移到 `agent-types`）
- 移除 AgentTool trait 和 AgentRuntimeDelegate trait（已迁移到 `agent-types`）
- 改为 re-export `agentdash-agent-types` 的全部 pub 类型
- 仅保留 SPI 独有内容：`AgentConnector` trait、`ExecutionContext`、Hook 系统（`SessionHookSnapshot` 等）

### 重构 `agentdash-agent`

- 依赖从 `agentdash-spi + rig-core` → **仅 `agentdash-agent-types`**
- `types.rs` 改为 re-export `agent-types`（不再 re-export spi）
- `bridge.rs`：
  - `BridgeRequest.tools` 改用 `agent_types::ToolDefinition`
  - 移除 `BridgeRequest.llm_messages`（bridge 实现自行转换）
  - `BridgeResponse.raw_content` 改为 `Vec<ContentPart>`（去 rig 类型）
  - `BridgeResponse.usage` 改为 `TokenUsage`
  - 移除 `RigBridge<M>` 实现
- `agent_loop.rs`：
  - 移除 `use rig::*`，改用自有 `ToolDefinition`
  - 内部用 `HashMap<String, DynAgentTool>` 持有工具实例
  - 移除与 delegate 重复的 `before_tool_call` / `after_tool_call` 独立回调
- `convert.rs`：整个文件搬到 executor
- Cargo.toml：移除 rig-core、agentdash-spi

### LLM Bridge 实现下沉到 `agentdash-executor`

- `RigBridge<M>` 和 `convert.rs`（AgentMessage ↔ rig::Message）搬到 `agentdash-executor/connectors/pi_agent/`
- `BridgeRequest.llm_messages` 由 RigBridge 内部通过 convert 生成，不再穿透到 agent 层

## 关键设计点

### AgentContext 双态设计

`agent-types::AgentContext` 仅持有 schema 快照，用于 DTO 传递：
```rust
pub struct AgentContext {
    pub system_prompt: String,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<ToolDefinition>,  // 仅 schema，不持有实例
}
```
Agent loop 内部另持有 `tool_registry: HashMap<String, DynAgentTool>` 用于实际执行。构建 DTO 时从工具实例列表生成 `AgentContext`。

### BridgeResponse 去 rig 化

```rust
pub struct BridgeResponse {
    pub message: AgentMessage,
    pub raw_content: Vec<ContentPart>,  // 替代 Vec<rig::AssistantContent>
    pub usage: TokenUsage,              // 替代 rig::Usage
}
```

### ToolDefinition 新类型

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON Schema
}
```

## 分阶段实施

### Phase 1: 创建 `agentdash-agent-types` crate

1. 在 workspace Cargo.toml 中添加新成员
2. 创建 crate 结构，从 `agentdash-spi` 提取纯数据类型
3. 迁移 `AgentTool` trait 和 `AgentRuntimeDelegate` trait
4. 新增 `ToolDefinition` 类型
5. 确保编译通过

### Phase 2: 重构 `agentdash-spi` → 依赖 `agent-types`

1. 添加 `agentdash-agent-types` 依赖
2. 将 `spi::tool` 和 `spi::lifecycle` 中的已迁移类型替换为 re-export
3. 清理不再需要的直接依赖（async-trait 等可通过 agent-types 传递）
4. 确保 `cargo check --workspace` 通过

### Phase 3: 重构 `agentdash-agent` → 依赖 `agent-types`，移除 rig

1. `types.rs` 改为 re-export `agent-types`
2. `bridge.rs` 中 `BridgeRequest` 改用 `ToolDefinition`，移除 `llm_messages`
3. `agent_loop.rs` 移除 `use rig::*`，改用 `ToolDefinition`
4. 将 `RigBridge` 和 `convert.rs` 搬到 `agentdash-executor/connectors/pi_agent/`
5. Cargo.toml 移除 rig-core、agentdash-spi 依赖

### Phase 4: 修复下游依赖

1. `agentdash-executor`: pi_agent connector 中引入搬来的 `RigBridge` + `convert`
2. `agentdash-application`: 调整 import 路径
3. `agentdash-api`: 调整 import 路径
4. `agentdash-local`: 调整 import 路径
5. 全量编译验证

### Phase 5: 清理 re-export 链

1. 各 crate 的 re-export 统一：消费者应从 `agent-types` 直接导入纯类型
2. `agentdash-spi` 的 re-export 保留用于向后兼容
3. 更新 backend spec 文档中的依赖关系图
4. `cargo check --workspace` + `cargo clippy --workspace -- -D warnings`

## Acceptance Criteria

- [ ] `agentdash-agent-types` crate 存在且仅依赖 serde + thiserror + async-trait + tokio-util
- [ ] `agentdash-agent` 不再依赖 rig-core 和 agentdash-spi
- [ ] `agentdash-agent` 仅依赖 `agentdash-agent-types` + tokio + async 运行时
- [ ] `RigBridge` 和 `convert.rs` 位于 `agentdash-executor` 中
- [ ] `agentdash-spi` 的 trait 签名引用 `agent-types` 中的类型
- [ ] `cargo check --workspace` 通过
- [ ] `cargo test --workspace` 通过
- [ ] `cargo clippy --workspace -- -D warnings` 通过

## Technical Notes

- `agentdash-agent` 的 `types.rs` 通过 re-export 保持向后兼容，Phase 5 可逐步迁移消费者
- `agentdash-spi` 中的 `CancellationToken`（来自 tokio-util）现在由 agent-types 统一承载
- `LlmBridge` trait 定义留在 `agentdash-agent` 中（它是 Agent Loop 的 port），`RigBridge` 是 adapter 搬到 executor
- Hook 状态类型（`SessionHookSnapshot` 等）留在 SPI 而非 agent-types，因为它们是 AgentDash 平台特有概念
- `agentdash-agent` 中的内置工具（ReadFile, WriteFile, Shell, Search, ListDirectory）留在 agent crate，它们实现 `agent-types::AgentTool` trait
- `AgentContext.tools` 改为 `Vec<ToolDefinition>` 后，agent_loop 需要维护独立的 tool registry 来查找并执行工具
