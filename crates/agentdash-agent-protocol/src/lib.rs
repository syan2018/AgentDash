pub mod backbone;
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

pub use codex_app_server_protocol;

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
        ProviderAttemptPhase, ProviderAttemptStatus, RuntimeTerminalDiagnostic,
        SessionRewindReason, SessionRewound,
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
    fn provider_status_platform_event_uses_snake_case_contract() {
        let event = BackboneEvent::Platform(PlatformEvent::ProviderAttemptStatus(
            ProviderAttemptStatus {
                turn_id: "turn-1".to_string(),
                phase: ProviderAttemptPhase::Retrying,
                attempt: 2,
                max_attempts: 3,
                will_retry: true,
                delay_ms: Some(2_000),
                reason_code: Some("stream_disconnected".to_string()),
                message: Some("Reconnecting... 2/3".to_string()),
                provider: Some("openai".to_string()),
                model: Some("gpt-4.1".to_string()),
            },
        ));

        let value = serde_json::to_value(event).expect("serialize provider platform event");
        assert_eq!(value["type"], "platform");
        assert_eq!(value["payload"]["kind"], "provider_attempt_status");
        assert_eq!(value["payload"]["data"]["turn_id"], "turn-1");
        assert_eq!(value["payload"]["data"]["phase"], "retrying");
        assert_eq!(value["payload"]["data"]["max_attempts"], 3);
        assert_eq!(value["payload"]["data"]["will_retry"], true);
        assert_eq!(value["payload"]["data"]["delay_ms"], 2_000);
        assert_eq!(value["payload"]["data"]["provider"], "openai");
        assert_eq!(value["payload"]["data"]["model"], "gpt-4.1");
    }

    #[test]
    fn runtime_terminal_diagnostic_platform_event_uses_typed_contract() {
        let event = BackboneEvent::Platform(PlatformEvent::RuntimeTerminalDiagnostic(
            RuntimeTerminalDiagnostic {
                kind: "provider".to_string(),
                code: Some("invalid_request".to_string()),
                http_status: Some(400),
                provider: Some("Example LLM".to_string()),
                model: Some("example-chat-large".to_string()),
                message: "provider returned 400".to_string(),
                retryable: false,
            },
        ));

        let value = serde_json::to_value(event).expect("serialize diagnostic platform event");
        assert_eq!(value["type"], "platform");
        assert_eq!(value["payload"]["kind"], "runtime_terminal_diagnostic");
        assert_eq!(value["payload"]["data"]["kind"], "provider");
        assert_eq!(value["payload"]["data"]["code"], "invalid_request");
        assert_eq!(value["payload"]["data"]["http_status"], 400);
        assert_eq!(value["payload"]["data"]["provider"], "Example LLM");
        assert_eq!(value["payload"]["data"]["model"], "example-chat-large");
        assert_eq!(value["payload"]["data"]["retryable"], false);
    }

    #[test]
    fn control_plane_projection_changed_platform_event_uses_typed_contract() {
        let event = BackboneEvent::Platform(PlatformEvent::ControlPlaneProjectionChanged(
            ControlPlaneProjectionChanged {
                projection: ControlPlaneProjection::Workspace,
                reason: ControlPlaneProjectionChangeReason::MailboxStateChanged,
                run_id: "run-1".to_string(),
                agent_id: "agent-1".to_string(),
                frame_id: Some("frame-1".to_string()),
                gate_id: Some("gate-1".to_string()),
                mailbox_message_id: Some("mailbox-1".to_string()),
                delivery_runtime_session_id: Some("session-1".to_string()),
                workspace_module_presentation: None,
            },
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
        assert_eq!(
            value["payload"]["data"]["delivery_runtime_session_id"],
            "session-1",
        );
    }

    #[test]
    fn session_rewound_platform_event_uses_stable_boundary_contract() {
        let event = BackboneEvent::Platform(PlatformEvent::SessionRewound(SessionRewound {
            discarded_turn_id: "turn-failed".to_string(),
            discarded_entry_index: Some(1),
            stable_event_seq: 120,
            stable_turn_id: Some("turn-stable".to_string()),
            reason: SessionRewindReason::ProviderFailure,
            replacement_turn_id: None,
            message: Some("rewound failed turn".to_string()),
        }));

        let value = serde_json::to_value(event).expect("serialize rewind platform event");
        assert_eq!(value["type"], "platform");
        assert_eq!(value["payload"]["kind"], "session_rewound");
        assert_eq!(value["payload"]["data"]["discarded_turn_id"], "turn-failed");
        assert_eq!(value["payload"]["data"]["discarded_entry_index"], 1);
        assert_eq!(value["payload"]["data"]["stable_event_seq"], 120);
        assert_eq!(value["payload"]["data"]["stable_turn_id"], "turn-stable");
        assert_eq!(value["payload"]["data"]["reason"], "provider_failure");
    }
}
