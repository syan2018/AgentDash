pub mod backbone;
pub mod generated;
pub mod thread_item;

// ─── 集中 re-export（保持外部 API 不变）───────────────────

pub use backbone::approval::ApprovalRequest;
pub use backbone::envelope::{BackboneEnvelope, SourceInfo, TraceInfo};
pub use backbone::event::BackboneEvent;
pub use backbone::item::{
    ItemCompletedNotification, ItemStartedNotification, ItemUpdatedNotification,
};
pub use backbone::platform::{
    ControlPlaneProjection, ControlPlaneProjectionChangeReason, ControlPlaneProjectionChanged,
    ControlPlaneWorkspaceModulePresentation, HookTraceCompletion, HookTraceData,
    HookTraceDiagnostic, HookTraceInjection, HookTracePayload, HookTraceSeverity, HookTraceTrigger,
    PlatformEvent, ProviderAttemptPhase, ProviderAttemptStatus, RuntimeTerminalDiagnostic,
    SessionRewindReason, SessionRewound,
};
pub use backbone::usage::{
    ContextUsageSource, NormalizedContextUsage, ThreadTokenUsage,
    ThreadTokenUsageUpdatedNotification, TokenUsageBreakdown,
};
pub use backbone::user_input::{
    UserInputBlock, UserInputConversionError, UserInputSource, UserInputSubmissionKind,
    UserInputSubmittedNotification, codex_user_input_to_text, content_block_to_codex_user_input,
    content_blocks_to_codex_user_input, text_user_input_block, text_user_input_blocks,
    user_input_blocks_to_content_parts, user_input_text,
};

/// AgentDash-owned Codex-shaped protocol facade.
pub mod codex_app_server_protocol {
    pub use crate::generated::codex_v2::command_execution_request_approval_params::CommandExecutionRequestApprovalParams;
    pub use crate::generated::codex_v2::file_change_request_approval_params::FileChangeRequestApprovalParams;
    pub use crate::generated::codex_v2::permissions_request_approval_params::PermissionsRequestApprovalParams;
    pub use crate::generated::codex_v2::server_notification::{
        AgentMessageDeltaNotification, CommandExecutionOutputDeltaNotification,
        ConfigWarningNotification, ContextCompactedNotification, DeprecationNoticeNotification,
        ErrorNotification, FileChangeOutputDeltaNotification, FileChangePatchUpdatedNotification,
        GuardianWarningNotification, ItemCompletedNotification,
        ItemGuardianApprovalReviewCompletedNotification,
        ItemGuardianApprovalReviewStartedNotification, ItemStartedNotification,
        McpToolCallProgressNotification, ModelReroutedNotification,
        ModelSafetyBufferingUpdatedNotification, ModelVerificationNotification,
        PlanDeltaNotification, ReasoningSummaryPartAddedNotification,
        ReasoningSummaryTextDeltaNotification, ReasoningTextDeltaNotification, RequestId,
        ServerRequestResolvedNotification, TerminalInteractionNotification,
        ThreadStatusChangedNotification, ThreadTokenUsage, ThreadTokenUsageUpdatedNotification,
        TokenUsageBreakdown, Turn, TurnCompletedNotification, TurnDiffUpdatedNotification,
        TurnError, TurnModerationMetadataNotification, TurnPlanStep, TurnPlanStepStatus,
        TurnPlanUpdatedNotification, TurnStartedNotification, TurnStatus, WarningNotification,
    };
    pub use crate::generated::codex_v2::thread_item::*;
    pub use crate::generated::codex_v2::tool_request_user_input_params::ToolRequestUserInputParams;
}

pub use thread_item::{
    AgentDashNativeThreadItem, AgentDashThreadItem, CodexThreadItem, CommandExecutionStatus,
    DynamicToolCallOutputContentItem, DynamicToolCallStatus, McpToolCallStatus, PatchApplyStatus,
    ShellExecExecutionMode,
};

pub use agent_client_protocol::{ContentBlock, EmbeddedResourceResource, TextContent};

#[cfg(test)]
mod tests {
    use std::fs;

    use ts_rs::TS;

    use super::{
        BackboneEnvelope, BackboneEvent, ControlPlaneProjection,
        ControlPlaneProjectionChangeReason, ControlPlaneProjectionChanged, PlatformEvent,
    };

    #[test]
    fn backbone_types_export_to_explicit_temp_dir() {
        let dir = tempfile::tempdir().expect("create temp dir");

        BackboneEnvelope::export_all_to(dir.path()).expect("export backbone protocol types");

        assert!(dir.path().join("BackboneEnvelope.ts").exists());
        assert!(dir.path().join("BackboneEvent.ts").exists());
        assert!(dir.path().join("PlatformEvent.ts").exists());

        let generated = fs::read_to_string(dir.path().join("BackboneEnvelope.ts"))
            .expect("read generated envelope type");
        assert!(generated.contains("export type BackboneEnvelope"));
    }

    #[test]
    fn control_plane_projection_changed_platform_event_uses_typed_contract() {
        let event = BackboneEvent::Platform(PlatformEvent::ControlPlaneProjectionChanged(
            Box::new(ControlPlaneProjectionChanged {
                projection: ControlPlaneProjection::Workspace,
                reason: ControlPlaneProjectionChangeReason::MailboxStateChanged,
                run_id: "run-1".to_string(),
                agent_id: "agent-1".to_string(),
                frame_id: Some("frame-1".to_string()),
                gate_id: Some("gate-1".to_string()),
                mailbox_message_id: Some("mailbox-1".to_string()),
                delivery_runtime_session_id: None,
                workspace_module_presentation: None,
            }),
        ));

        let value =
            serde_json::to_value(event).expect("serialize control-plane projection platform event");
        assert_eq!(value["type"], "platform");
        assert_eq!(value["payload"]["kind"], "control_plane_projection_changed");
        assert_eq!(value["payload"]["data"]["projection"], "workspace");
        assert_eq!(value["payload"]["data"]["reason"], "mailbox_state_changed");
        assert_eq!(value["payload"]["data"]["run_id"], "run-1");
        assert_eq!(value["payload"]["data"]["agent_id"], "agent-1");
        assert_eq!(value["payload"]["data"]["frame_id"], "frame-1");
        assert_eq!(value["payload"]["data"]["gate_id"], "gate-1");
        assert_eq!(value["payload"]["data"]["mailbox_message_id"], "mailbox-1");
    }

    #[test]
    fn backbone_json_schema_support_does_not_change_serde_shape() {
        let fixture = serde_json::json!({
            "type": "item_completed",
            "payload": {
                "item": {
                    "type": "dynamicToolCall",
                    "id": "item-1",
                    "namespace": null,
                    "tool": "fixture",
                    "arguments": { "nullable": null },
                    "status": "completed",
                    "contentItems": null,
                    "success": true,
                    "durationMs": null
                },
                "threadId": "thread-1",
                "turnId": "turn-1",
                "completedAtMs": 123_i64
            }
        });
        let event: BackboneEvent =
            serde_json::from_value(fixture.clone()).expect("deserialize backbone fixture");
        assert_eq!(
            serde_json::to_value(event).expect("serialize backbone fixture"),
            fixture
        );
    }
}
