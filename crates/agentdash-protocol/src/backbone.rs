use codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};

use crate::approval::ApprovalRequest;
use crate::platform::PlatformEvent;

/// 平台内部事件流转的统一类型。
///
/// 变体名由平台定义（控制语义），payload 类型严格对齐 Codex App Server Protocol。
/// 所有 connector（codex_bridge / pi_agent / vibe_kanban 等）都必须映射到同一套变体，
/// 不设"通用退化变体"。Codex 原生协议没有覆盖的语义通过 `Platform` 扩展。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BackboneEvent {
    // ── 文本 / 推理流 ──

    AgentMessageDelta(codex::AgentMessageDeltaNotification),
    ReasoningTextDelta(codex::ReasoningTextDeltaNotification),
    ReasoningSummaryDelta(codex::ReasoningSummaryTextDeltaNotification),

    // ── Item 生命周期（涵盖所有工具调用语义）──
    // ThreadItem 区分: CommandExecution / FileChange / McpToolCall /
    //   DynamicToolCall / AgentMessage / Plan / Reasoning / WebSearch 等

    ItemStarted(codex::ItemStartedNotification),
    ItemCompleted(codex::ItemCompletedNotification),

    // ── Item 过程增量 ──

    CommandOutputDelta(codex::CommandExecutionOutputDeltaNotification),
    FileChangeDelta(codex::FileChangeOutputDeltaNotification),
    McpToolCallProgress(codex::McpToolCallProgressNotification),

    // ── Turn 生命周期 ──

    TurnStarted(codex::TurnStartedNotification),
    TurnCompleted(codex::TurnCompletedNotification),
    TurnDiffUpdated(codex::TurnDiffUpdatedNotification),

    // ── Plan ──

    TurnPlanUpdated(codex::TurnPlanUpdatedNotification),
    PlanDelta(codex::PlanDeltaNotification),

    // ── 资源 / 状态 ──

    TokenUsageUpdated(codex::ThreadTokenUsageUpdatedNotification),
    ThreadStatusChanged(codex::ThreadStatusChangedNotification),
    ContextCompacted(codex::ContextCompactedNotification),

    // ── 审批请求（server → client，需要平台决策后回传）──

    ApprovalRequest(ApprovalRequest),

    // ── 错误 ──

    Error(codex::ErrorNotification),

    // ── 平台扩展（Codex 原生协议没有的能力）──

    Platform(PlatformEvent),
}
