# AgentTool SPI 下沉到 connector-contract + Pi Agent 定位重构

## Goal

将 `AgentTool` trait 族从 `agentdash-agent` 下沉到 `agentdash-connector-contract`，
使其成为跨层工具合同；同时明确 `agentdash-agent` 作为"Pi Agent Runtime"的定位，
与 vibe-kanban 平级，是一个可选执行器实现。

## Background

### 核心洞察

`agentdash-agent` 不是基础设施 SDK——它是一个独立的 Bounded Context（Agent 领域），
包含自己的领域模型（AgentMessage, AgentState, AgentEvent）、领域契约（AgentTool,
AgentRuntimeDelegate）和应用逻辑（Agent lifecycle, agent_loop）。

但其中 `AgentTool` trait 族的定位更接近**跨层工具合同 SPI**——任何 agent runtime
（Pi Agent、vibe-kanban、未来的远程 ACP agent）都需要一套统一的工具接口。

### 当前依赖关系

```
connector-contract  ← 合同层（AgentConnector, ExecutionHookProvider, ExecutionContext...）
     ↑                 不依赖 agent
     │
executor            ← 编排层（ExecutorHub, connectors/）
     │                 agent 通过 feature "pi-agent" 可选依赖
     │                 RuntimeToolProvider → Vec<DynAgentTool> [cfg(pi-agent)]
     ↑
application         ← 业务用例层（不依赖 agent，不依赖 executor 的 pi-agent feature）
     ↑
api                 ← 组合层（依赖 agent + application + executor）
                      tool 实现在此（实现 agent::AgentTool）

agent               ← 独立岛（零 agentdash 依赖！只依赖 rig-core）
```

### 问题

1. `application` 无法实现 `AgentTool`（不依赖 agent）→ tool 实现被迫留在 api
2. `RuntimeToolProvider` 需要 `#[cfg(feature = "pi-agent")]` gate
3. `ThinkingLevel` 在 connector-contract 和 agent 中重复定义
4. agent 的定位模糊——既像 infra SDK 又像独立领域

## 方案：AgentTool SPI 下沉到 connector-contract

### 目标依赖关系

```
connector-contract  (新增: AgentTool, ContentPart, AgentToolResult, AgentToolError, DynAgentTool)
     ↑                    ↑
     │                    │
executor              agent (新增: 首次依赖 connector-contract，获取 tool SPI)
     │                    Pi Agent loop/bridge/runtime 留在这里
     ↑                    作为可选执行器，与 vibe-kanban 平级
application
     ↑
api (仅做组合：构建 tool 实例，注入 executor)
```

### 迁移内容

从 `agentdash-agent/src/types.rs` 迁入 `connector-contract/src/tool.rs`：

| 类型 | 说明 |
|------|------|
| `AgentTool` trait | 工具执行接口 |
| `AgentToolResult` | 工具执行结果 |
| `AgentToolError` | 工具错误类型 |
| `ContentPart` | 内容片段（Text/Image/Reasoning） |
| `DynAgentTool` | `Arc<dyn AgentTool>` 别名 |
| `ToolUpdateCallback` | 工具进度回调 |

**不迁移**（留在 agent，属于 Pi Agent 领域）：

| 类型 | 说明 |
|------|------|
| `AgentMessage` | Pi Agent 消息模型 |
| `AgentEvent` / `AgentState` | Pi Agent 状态/事件 |
| `AgentRuntimeDelegate` | Pi Agent 运行时委托 |
| `Agent` / `AgentConfig` | Pi Agent 生命周期 |
| `agent_loop` | Pi Agent 执行循环 |
| `LlmBridge` / `RigBridge` | Pi Agent LLM 桥接 |
| `BuiltinToolset` / `ToolRegistry` | Pi Agent 内置工具 |

### 对各 crate 的影响

| Crate | 变更 |
|-------|------|
| `connector-contract` | 新增 `tool.rs` 模块，定义 AgentTool SPI |
| `agent` | 新增依赖 `connector-contract`；types.rs 中 tool 相关类型改为 re-export |
| `executor` | `RuntimeToolProvider` 去掉 `#[cfg(pi-agent)]`；`connector.rs` 直接用合同层类型 |
| `application` | 已依赖 `connector-contract` → 自动获得 AgentTool SPI → 可实现 tool |
| `api` | tool 实现可迁出到 application（后续任务） |

### ThinkingLevel 三重定义统一

`ThinkingLevel` 当前在**三个 crate** 中各有一份完全相同的定义：

| 位置 | 当前状态 |
|------|---------|
| `connector-contract/src/connector.rs` | 合同层定义（权威来源） |
| `agent/src/types.rs` | 独立定义（与合同层重复） |
| `application/src/runtime.rs` | 独立定义（与合同层重复） |

下沉后统一为 connector-contract 中的单一定义：
- `agent` → `pub use agentdash_connector_contract::connector::ThinkingLevel`
- `application` → `pub use agentdash_connector_contract::connector::ThinkingLevel`（或通过 executor re-export）
- 消除 `to_agent_runtime_thinking_level()` 等所有转换函数

## Execution Plan

### Step 1: connector-contract 新增 tool.rs

```rust
// connector-contract/src/tool.rs
pub trait AgentTool: Send + Sync { ... }
pub struct AgentToolResult { ... }
pub enum AgentToolError { ... }
pub enum ContentPart { ... }
pub type DynAgentTool = Arc<dyn AgentTool>;
pub type ToolUpdateCallback = Arc<dyn Fn(AgentToolResult) + Send + Sync>;
```

### Step 2: agent 依赖 connector-contract

- Cargo.toml 新增 `agentdash-connector-contract = { workspace = true }`
- types.rs 中 tool 相关类型改为 re-export（保持 agent 对外 API 不变）
- ThinkingLevel 改为 re-export

### Step 3: application 清理 ThinkingLevel 重复定义

- `application/src/runtime.rs` 中删除本地 `ThinkingLevel` 定义
- 改为 `pub use agentdash_connector_contract::connector::ThinkingLevel`
  （或通过 executor re-export 路径：`pub use agentdash_executor::ThinkingLevel`）
- 确认 `ExecutorConfig` 中引用的 ThinkingLevel 指向统一定义

### Step 4: executor 清理 feature gate

- `RuntimeToolProvider` 去掉 `#[cfg(feature = "pi-agent")]`
- `connector.rs` 中 `to_agent_runtime_thinking_level()` 删除
- `pi_agent.rs` 改用统一 ThinkingLevel

### Step 5: 验证

- `cargo check` 全 crate 通过（含 `--no-default-features`）
- `cargo test` 全 crate 通过
- agent 对外 API 零 breaking change（re-export 保持兼容）

## Acceptance Criteria

- [ ] `AgentTool` trait 族定义在 `connector-contract/src/tool.rs`
- [ ] `agentdash-agent` 依赖 `connector-contract`，re-export tool 类型
- [ ] `RuntimeToolProvider` 无 feature gate
- [ ] `ThinkingLevel` 只有一份定义（在 connector-contract），agent 和 application 均为 re-export
- [ ] `cargo check` 全 crate 通过
- [ ] `cargo test` 全 crate 通过
- [ ] agent 对外 API 零 breaking change

## Priority

**整体重构链的第 1 步**（用户确认）。
本任务是所有后续迁移的前置条件——只有 SPI 下沉后，application 才能实现 AgentTool。

执行顺序：
1. **本任务：SPI 下沉**
2. hooks + services 迁移（并行）
3. tool 实现迁移
4. gateway 解构

## Dependency Chain

```
前置：03-27-api-god-module-decomposition (P0 文件拆分，已完成)
后续：03-27-migrate-execution-hooks-to-application (hooks 迁移)
后续：03-27-migrate-address-space-services-to-application (services + tool 迁移)
后续：03-27-abstract-task-gateway-context (gateway 解构)
```

## Risk

- **低风险**：connector-contract 是纯 trait/type crate，新增内容不引入运行时依赖
- **agent 首次依赖 agentdash 体系**：但只依赖最轻量的合同层，不破坏 agent 的独立性
- **re-export 兼容**：agent 对外 API 不变，所有 `use agentdash_agent::AgentTool` 继续工作
