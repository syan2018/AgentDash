# 领域负载类型化标准与盘点

## 1. 盘点：领域层 `serde_json::Value` 使用面

按使用场景分类，标注优先级（P0 = 高频路径，尽早类型化；P1 = 中频，逐步迁移；P2 = 低频/外部协议，可延期）。

### P0: 高频业务路径

| 文件 | 引用数 | 说明 | 类型化方向 |
|------|--------|------|-----------|
| `hooks/mod.rs` | 15 | Hook 事件 payload、rule match context、action payload | 定义 `HookEventPayload` 枚举 |
| `hooks/snapshot_helpers.rs` | 16 | Hook snapshot 序列化/反序列化中间体 | 用具体 snapshot 类型替代 |
| `task/tools/companion.rs` | 13 | Companion dispatch request/response payload | 定义 `CompanionPayload` 结构体 |
| `connectors/pi_agent.rs` | 12 | PiAgent tool call arguments / results | 工具面已有 schema，用 `ToolCallArgs` 包装 |
| `agent/entity.rs` | 11 | Agent 配置 extra_params | 定义 `AgentExtraParams` 结构体 |
| `routes/project_agents.rs` | 11 | Agent CRUD API payload | 应对齐 `agent/entity.rs` 类型 |

### P1: 中频路径

| 文件 | 引用数 | 说明 |
|------|--------|------|
| `hooks/rules.rs` | 4 | Rule condition evaluation context |
| `workflow/completion.rs` | 7 | Workflow artifact & completion payload |
| `connector-contract/hooks.rs` | 10 | Hook contract 公共接口 |
| `connector-contract/tool.rs` | 3 | Tool input/output schema |
| `agent/tools/builtins.rs` | 10 | 内置工具参数/结果 |
| `settings.rs` | 3 | Settings value (用户可编辑配置) |
| `mcp/servers/story.rs` | 5 | MCP story server payload |

### P2: 低频 / 外部协议边界

| 文件 | 引用数 | 说明 |
|------|--------|------|
| `relay/protocol.rs` | 4 | Relay 协议扩展字段 |
| `connectors/pi_agent_mcp.rs` | 6 | MCP 协议适配层 |
| `routes/workflows.rs` | 2 | Workflow HTTP API |
| `dto/workflow.rs` | 2 | Workflow DTO |
| `plugins.rs` / `plugin-api` | 2 | 插件接口（外部扩展点，刻意保持灵活） |
| 其他各 `routes/` | 少量 | API 层 DTO 与 domain 对齐即可 |

---

## 2. 结构化错误边界标准

### 2.1 原则

领域层错误**必须**是结构化的，不允许裸字符串错误传播到调用方。

### 2.2 标准错误枚举模板

```rust
/// 领域操作错误 — 所有变体必须携带足够的上下文供上层做日志/展示/重试决策
#[derive(Debug, thiserror::Error)]
pub enum DomainOperationError {
    /// 输入验证失败
    #[error("validation failed: {field} — {reason}")]
    Validation {
        field: String,
        reason: String,
    },

    /// 资源未找到
    #[error("not found: {resource_type} [{resource_id}]")]
    NotFound {
        resource_type: &'static str,
        resource_id: String,
    },

    /// 状态冲突（乐观锁、生命周期不匹配）
    #[error("conflict: {description}")]
    Conflict {
        description: String,
    },

    /// 外部依赖失败（relay、LLM、MCP）
    #[error("external dependency failed: {service} — {detail}")]
    ExternalFailure {
        service: String,
        detail: String,
        /// 是否可安全重试
        retryable: bool,
    },

    /// 内部错误（不应暴露给最终用户）
    #[error("internal: {0}")]
    Internal(String),
}
```

### 2.3 错误边界规则

| 层级 | 允许的错误类型 | 禁止 |
|------|---------------|------|
| Domain | 领域枚举 (`DomainOperationError`, `WorkflowError`, ...) | 裸 `String`、`anyhow::Error` 直传 |
| Application | 领域枚举 + `std::io::Error`（仅 I/O 操作） | 裸 `.to_string()` 后 wrap |
| API | `ApiError`（HTTP 语义映射） | 领域枚举直接序列化给前端 |
| Connector | `ConnectorError`（已定义） | `Box<dyn Error>` |

### 2.4 Workflow 校验错误标准

当前 workflow 校验错误多为裸字符串。标准替换方向：

```rust
#[derive(Debug, thiserror::Error, Serialize)]
pub enum WorkflowValidationError {
    #[error("missing required field: {field} in step {step_key}")]
    MissingField {
        step_key: String,
        field: String,
    },

    #[error("invalid transition: {from} -> {to} in lifecycle {lifecycle_id}")]
    InvalidTransition {
        lifecycle_id: String,
        from: String,
        to: String,
    },

    #[error("artifact not found: {artifact_key} in run {run_id}")]
    ArtifactNotFound {
        run_id: String,
        artifact_key: String,
    },

    #[error("completion criteria not met: {reason}")]
    CompletionNotMet {
        reason: String,
        context: serde_json::Value,  // 保留灵活性用于调试
    },
}
```

---

## 3. 类型化改造样板：Hook Event Payload

### 3.1 改造前（现状）

```rust
// hooks/mod.rs
pub struct HookTriggerEvent {
    pub trigger: String,
    pub payload: serde_json::Value,  // 裸 Value
}
```

### 3.2 改造后（目标）

```rust
/// Hook 触发事件 — payload 按 trigger 类型区分
pub struct HookTriggerEvent {
    pub trigger: HookTriggerKind,
    pub payload: HookEventPayload,
}

/// 类型化的 Hook 事件 payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HookEventPayload {
    /// 工具调用完成
    ToolCallCompleted {
        tool_name: String,
        tool_call_id: String,
        mount_id: Option<String>,
        path: Option<String>,
    },
    /// Turn 开始
    TurnStart {
        turn_id: String,
    },
    /// Turn 结束
    TurnEnd {
        turn_id: String,
        terminal_message: Option<String>,
    },
    /// 子代理分发
    SubagentDispatched {
        subagent_type: String,
        session_id: String,
    },
    /// 通用扩展（兜底，逐步收窄）
    Generic {
        data: serde_json::Value,
    },
}
```

### 3.3 迁移策略

1. **向后兼容**: `Generic` 变体保留 `serde_json::Value`，确保未覆盖的 trigger 不丢失
2. **逐步收窄**: 每次新增具体变体时，从 `Generic` 中提取该类型，旧数据仍可反序列化
3. **serde tag**: 使用 `#[serde(tag = "kind")]` 内部标签，保持 JSON 序列化兼容
4. **不强制一次性迁移**: 改造以模块为单位推进，每次只处理一个高频模块

---

## 4. 迁移优先级建议

| 阶段 | 模块 | 预期收益 |
|------|------|---------|
| Phase 1 | `hooks/mod.rs` + `hooks/rules.rs` | Hook 系统是最高频交互路径，类型化后 IDE 补全和编译时校验立即生效 |
| Phase 2 | `agent/entity.rs` + `routes/project_agents.rs` | Agent 配置是用户直接编辑的数据，类型安全直接提升用户面可靠性 |
| Phase 3 | `workflow/completion.rs` | Workflow 校验链是稳定化阶段的核心，结构化错误直接降低排查成本 |
| Phase 4 | `connector-contract/tool.rs` + `pi_agent.rs` | 工具面类型化需要协调 connector 契约，建议在 connector 升级时一并处理 |
| Deferred | `plugin-api`, `mcp/servers/` | 外部扩展点刻意保持 Value 灵活性，暂不强制类型化 |
