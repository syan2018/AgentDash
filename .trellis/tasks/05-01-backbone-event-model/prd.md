# backbone 事件模型定义 + Codex 映射规则

## Goal

定义平台内部事件类型体系，替换 ACP `SessionNotification` / `SessionUpdate` 作为内部事件流转基础。完成 Codex App Server Protocol 事件到内部类型的映射规则。

## 背景

当前 `ExecutionStream` 的类型签名是 `Stream<Item = Result<SessionNotification, ConnectorError>>`，connector 层产出的事件必须先转换为 ACP 类型。这导致：

1. 信息损耗：Codex 丰富的 item/command/fileChange 事件无法结构化表达
2. 语义错配：ACP 的 `ContentChunk` 只能承载文本/图片，不能表达工具调用过程
3. 全链路耦合：session hub、persistence、前端都绑定在 ACP 类型上

## 设计原则

1. **薄包裹 Codex 类型**：使用自定义 enum 变体名，但内部 payload 类型直接复用 Codex crate 的结构体
2. **平台掌控变体集**：哪些事件进入 BackboneEvent 由平台决定，而非直接 re-export 整个 Codex enum
3. **只加不改**：在 Codex 类型基础上添加平台 envelope，不修改 Codex 原始结构
4. **前端可消费**：事件类型必须能通过 rs-ts 直出到 TS

## 事件模型设计

### 核心类型：BackboneEvent（薄包裹方案）

```rust
use codex_app_server_protocol as codex;

/// 平台内部事件流转的统一类型。
///
/// 变体名由平台定义（控制语义），payload 类型复用 Codex crate。
/// 这样平台可以：
/// - 选择性接入 Codex 事件（不需要的不加变体）
/// - 添加平台自有事件（PlatformEvent）
/// - 未来替换某个变体的 payload 类型时只改一处
pub enum BackboneEvent {
    // —— 文本 / 推理流 ——
    AgentMessageDelta(codex::AgentMessageDeltaNotification),
    ReasoningTextDelta(codex::ReasoningTextDeltaNotification),
    ReasoningSummaryDelta(codex::ReasoningSummaryTextDeltaNotification),

    // —— Item 生命周期 ——
    ItemStarted(codex::ItemStartedNotification),
    ItemCompleted(codex::ItemCompletedNotification),

    // —— 工具调用 / 文件变更过程 ——
    CommandOutputDelta(codex::CommandExecutionOutputDeltaNotification),
    FileChangeDelta(codex::FileChangeOutputDeltaNotification),

    // —— Turn 生命周期 ——
    TurnStarted(codex::TurnStartedNotification),
    TurnCompleted(codex::TurnCompletedNotification),
    TurnDiffUpdated(codex::TurnDiffUpdatedNotification),

    // —— 资源 / 状态 ——
    TokenUsageUpdated(codex::ThreadTokenUsageUpdatedNotification),
    ThreadStatusChanged(codex::ThreadStatusChangedNotification),
    ContextCompacted(codex::ContextCompactedNotification),

    // —— 审批请求（server → client，需要平台决策后回传） ——
    ApprovalRequest(ApprovalRequest),

    // —— 错误 ——
    Error(codex::ErrorNotification),

    // —— 平台自有事件 ——
    Platform(PlatformEvent),
}

/// 审批请求的薄包裹（保留 request_id 用于回传）
pub enum ApprovalRequest {
    CommandExecution {
        request_id: codex::RequestId,
        params: codex::CommandExecutionRequestApprovalParams,
    },
    FileChange {
        request_id: codex::RequestId,
        params: codex::FileChangeRequestApprovalParams,
    },
    ToolUserInput {
        request_id: codex::RequestId,
        params: codex::ToolRequestUserInputParams,
    },
}

/// 平台自有事件
pub enum PlatformEvent {
    ExecutorSessionBound { executor_session_id: String },
    // 后续可扩展
}

/// 平台 envelope — 包裹每条事件
pub struct BackboneEnvelope {
    pub event: BackboneEvent,
    pub trace_id: String,
    pub source_connector: String,
    pub observed_at: DateTime<Utc>,
    pub protocol_version: String,
}
```

### ExecutionStream 替换

```rust
// 当前
pub type ExecutionStream = Pin<Box<dyn Stream<Item = Result<SessionNotification, ConnectorError>> + Send>>;

// 目标
pub type ExecutionStream = Pin<Box<dyn Stream<Item = Result<BackboneEnvelope, ConnectorError>> + Send>>;
```

### 与 Codex enum 的关系

| 方面 | 直接 re-export | 薄包裹（推荐） |
|------|---------------|---------------|
| Codex 新增事件 | 自动进入平台 | 平台显式选择接入 |
| 变体命名 | 受 Codex 控制 | 平台自行命名 |
| 替换某个 payload | 需要包裹整个 enum | 只改一个变体 |
| 代码量 | 最少 | 稍多但可控 |
| 类型耦合 | 强（平台内直接使用 Codex enum） | 中（变体层平台持有，payload 层复用 Codex） |

### Codex 事件映射规则

| Codex 通知方法 | 映射策略 | 优先级 |
|---|---|---|
| `item/agentMessage/delta` | 直接透传 | P0 |
| `item/reasoning/textDelta` | 直接透传 | P0 |
| `item/reasoning/summaryTextDelta` | 直接透传 | P0 |
| `thread/tokenUsage/updated` | 直接透传 | P0 |
| `turn/completed` | 直接透传 | P0 |
| `error` | 直接透传 | P0 |
| `item/started` | 直接透传 | P0 |
| `item/completed` | 直接透传 | P0 |
| `item/commandExecution/outputDelta` | 直接透传 | P0 |
| `item/fileChange/outputDelta` | 直接透传 | P0 |
| `turn/started` | 直接透传 | P0 |
| `turn/diff/updated` | 直接透传 | P0 |
| `item/mcpToolCall/progress` | 直接透传 | P1 |
| `turn/plan/updated` | 直接透传 | P1 |
| `item/plan/delta` | 直接透传 | P1 |
| `thread/status/changed` | 直接透传 | P1 |
| `thread/compacted` | 直接透传 | P1 |

| Codex 请求方法 | 映射策略 | 优先级 |
|---|---|---|
| `item/commandExecution/requestApproval` | 透传 + 挂起等待平台决策 | P1 |
| `item/fileChange/requestApproval` | 透传 + 挂起等待平台决策 | P1 |
| `item/tool/requestUserInput` | 透传 + 挂起等待用户输入 | P2 |
| `item/permissions/requestApproval` | 透传 + 挂起等待平台决策 | P2 |

## Acceptance Criteria

* [ ] 定义 `BackboneEvent` 枚举类型并可编译
* [ ] 定义 `BackboneEnvelope` envelope 类型
* [ ] 替换 `ExecutionStream` 类型签名（或提供渐进迁移路径）
* [ ] 映射表覆盖所有 P0 事件
* [ ] 形成从 `BackboneEvent` 到当前 ACP `SessionNotification` 的临时兼容转换（过渡期用）

## Out of Scope

* 不修改 session hub / persistence 层的消费代码（由后续 ACP 退出任务处理）
* 不实现 conformance 自动化测试
* 不定义多执行器统一映射（首期只做 Codex）
