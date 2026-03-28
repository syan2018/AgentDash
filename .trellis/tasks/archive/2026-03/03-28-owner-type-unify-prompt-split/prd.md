# Owner 类型统一映射与 PromptSessionRequest 拆分

## Goal

1. 消除三套并行的 Owner 类型枚举（`SessionOwnerType`、`SessionPlanOwnerKind`、`SessionMountTarget`），建立统一映射
2. 将 `PromptSessionRequest` 中混杂的用户输入字段与后端注入字段分离为不同结构体

## 背景

### 问题 S1 — 三套 Owner 枚举

| 枚举 | 位置 | 变体 |
|------|------|------|
| `SessionOwnerType` | `agentdash-domain/src/session_binding/value_objects.rs` | `Project`, `Story`, `Task` |
| `SessionPlanOwnerKind` | `agentdash-application/src/session_plan.rs` | `ProjectAgent`, `TaskExecution`, `StoryOwner` |
| `SessionPlanPhase` | `agentdash-application/src/session_plan.rs` | `ProjectAgent`, `TaskStart`, `TaskContinue`, `StoryOwner` |

这三者语义重叠但命名不同，且 `SessionPlanPhase` 额外区分了 `TaskStart`/`TaskContinue`，增加了映射的心智负担。

### 问题 S2 — PromptSessionRequest 是"super DTO"

`PromptSessionRequest`（`agentdash-executor/src/hub.rs`）同时包含：
- **用户输入**：`prompt`, `prompt_blocks`, `working_dir`, `env`
- **后端注入**：`mcp_servers`, `workspace_root`, `address_space`, `flow_capabilities`, `system_context`（均 `#[serde(skip)]`）

这种设计导致：
- 从 HTTP handler 反序列化时，skip 字段被忽略
- 后端代码必须在反序列化后逐个"填充"skip 字段
- 无法在类型层面区分"已填充"和"未填充"的请求

## Requirements

### Part 1: 统一 Owner 映射

**方案**：保留 `SessionOwnerType`（Domain 层）作为唯一的归属枚举。`SessionPlanOwnerKind` 转为从 `SessionOwnerType` 派生的方法/关联函数。

具体改动：
1. 在 `SessionOwnerType` 上增加辅助方法：
   ```rust
   impl SessionOwnerType {
       pub fn default_plan_phase(&self, is_continuation: bool) -> SessionPlanPhase { ... }
   }
   ```
2. 删除 `SessionPlanOwnerKind` 枚举，将其使用处改为直接使用 `SessionOwnerType`
3. `SessionPlanPhase` 保留（因为它编码了 start/continue 区分），但改为由 `SessionOwnerType` + `is_continuation` 推导

### Part 2: 拆分 PromptSessionRequest

将当前 struct 拆为两层：

```rust
/// 纯用户输入 — HTTP 反序列化的目标
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptInput {
    pub prompt: Option<String>,
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    pub working_dir: Option<String>,
    pub env: HashMap<String, String>,
    pub executor_config: Option<AgentDashExecutorConfig>,
}

/// 后端完整请求 — 包含注入的运行时上下文
pub struct PromptSessionRequest {
    pub user_input: UserPromptInput,
    pub mcp_servers: Vec<McpServer>,
    pub workspace_root: Option<PathBuf>,
    pub address_space: Option<ExecutionAddressSpace>,
    pub flow_capabilities: Option<FlowCapabilities>,
    pub system_context: Option<String>,
}
```

- `UserPromptInput` 用于 HTTP handler 的 `Json<UserPromptInput>` 反序列化
- `PromptSessionRequest` 由 session bootstrap 代码构造（组合用户输入 + 后端注入）
- `resolve_prompt_payload()` 方法移至 `UserPromptInput`

## Acceptance Criteria

- [ ] `SessionPlanOwnerKind` 枚举被删除，所有使用处迁移到 `SessionOwnerType`
- [ ] `SessionPlanPhase` 改为由 `SessionOwnerType` + bool 推导
- [ ] `UserPromptInput` 和 `PromptSessionRequest` 分离
- [ ] `PromptSessionRequest` 不再有 `#[serde(skip)]` 字段
- [ ] HTTP handler 反序列化 `UserPromptInput` 而非 `PromptSessionRequest`
- [ ] 所有 test 通过，`cargo check --workspace` 无错误

## Technical Notes

- Part 1 和 Part 2 可以在同一个 PR 中，因为改动范围高度重叠
- `SessionMountTarget` 如果存在也需要一并清理（检查是否还在使用）
- relay 层通过 `CommandPromptPayload` 发送请求，需要确认其与 `UserPromptInput` 的对齐

## 依赖

- 无硬性前置，但建议在 `session-bootstrap-pipeline` 之前完成

## 优先级

P1 — 中高优先级，为 Session Pipeline 标准化铺路
