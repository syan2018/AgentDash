use codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::backbone::approval::ApprovalRequest;
use crate::backbone::item::{
    ItemCompletedNotification, ItemStartedNotification, ItemUpdatedNotification,
};
use crate::backbone::platform::PlatformEvent;
use crate::backbone::usage::ThreadTokenUsageUpdatedNotification;
use crate::backbone::user_input::UserInputSubmittedNotification;

/// 平台内部事件流转的统一类型。
///
/// 变体名由平台定义（控制语义），payload 优先对齐 Codex App Server Protocol。
/// 所有 connector（codex_bridge / pi_agent 等）都必须映射到同一套变体，
/// 不设"通用退化变体"。Codex 原生协议没有覆盖的 item 语义通过
/// `AgentDashThreadItem` 扩展，平台能力通过 `Platform` 扩展。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BackboneEvent {
    // ── 文本 / 推理流 ──
    AgentMessageDelta(codex::AgentMessageDeltaNotification),
    ReasoningTextDelta(codex::ReasoningTextDeltaNotification),
    ReasoningSummaryDelta(codex::ReasoningSummaryTextDeltaNotification),

    // ── Item 生命周期（涵盖所有工具调用语义）──
    // AgentDashThreadItem 区分 Codex 原生 item 与 AgentDash native item。
    ItemStarted(ItemStartedNotification),
    /// item 进度刷新（args/preview/partial output 精化）。区别于 `ItemStarted`
    /// 的 create-once 语义，`ItemUpdated` 表达同一 item_id 的后续刷新。
    ItemUpdated(ItemUpdatedNotification),
    ItemCompleted(ItemCompletedNotification),

    // ── Item 过程增量 ──
    CommandOutputDelta(codex::CommandExecutionOutputDeltaNotification),
    FileChangeDelta(codex::FileChangeOutputDeltaNotification),
    McpToolCallProgress(codex::McpToolCallProgressNotification),

    // ── Turn 生命周期 ──
    TurnStarted(codex::TurnStartedNotification),
    TurnCompleted(codex::TurnCompletedNotification),
    TurnDiffUpdated(codex::TurnDiffUpdatedNotification),

    // ── 用户输入（Codex UserInput + AgentDash submission 标注）──
    UserInputSubmitted(UserInputSubmittedNotification),

    // ── Plan ──
    TurnPlanUpdated(codex::TurnPlanUpdatedNotification),
    PlanDelta(codex::PlanDeltaNotification),

    // ── 资源 / 状态 ──
    TokenUsageUpdated(ThreadTokenUsageUpdatedNotification),
    ThreadStatusChanged(codex::ThreadStatusChangedNotification),
    /// 外部 executor 自行完成的 compact 标记。该事件没有 AgentDash-owned
    /// summary/boundary/replacement provenance，只能作为遥测与审计事实。
    ExecutorContextCompacted(codex::ContextCompactedNotification),

    // ── 审批请求（server → client，需要平台决策后回传）──
    ApprovalRequest(ApprovalRequest),

    // ── 错误 ──
    Error(codex::ErrorNotification),

    // ── 平台扩展（Codex 原生协议没有的能力）──
    Platform(PlatformEvent),
}
