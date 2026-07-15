pub mod backbone;
pub mod generated;
pub mod thread_item;

// ─── 集中 re-export（保持外部 API 不变）───────────────────

pub use backbone::approval::ApprovalRequest;
pub use backbone::context_frame::*;
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

mod transcript_projection;
pub use transcript_projection::{TranscriptProjectionEvent, project_transcript};

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
        BackboneEnvelope, BackboneEvent, ContextDeliveryMetadata, ContextFrame,
        ControlPlaneProjection, ControlPlaneProjectionChangeReason, ControlPlaneProjectionChanged,
        PlatformEvent,
    };

    #[test]
    fn backbone_types_export_to_explicit_temp_dir() {
        let dir = tempfile::tempdir().expect("create temp dir");

        BackboneEnvelope::export_all_to(dir.path()).expect("export backbone protocol types");

        assert!(dir.path().join("BackboneEnvelope.ts").exists());
        assert!(dir.path().join("BackboneEvent.ts").exists());
        assert!(dir.path().join("PlatformEvent.ts").exists());
        assert!(dir.path().join("ContextFrame.ts").exists());
        assert!(dir.path().join("ContextFrameSection.ts").exists());

        let generated = fs::read_to_string(dir.path().join("BackboneEnvelope.ts"))
            .expect("read generated envelope type");
        assert!(generated.contains("export type BackboneEnvelope"));
        let platform = fs::read_to_string(dir.path().join("PlatformEvent.ts"))
            .expect("read generated platform event type");
        assert!(platform.contains("context_frame_changed"));
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
    fn context_frame_changed_preserves_the_main_reference_payload_shape() {
        use super::{
            ContextDeliveryChannel, ContextDeliveryStatus, ContextFrame, ContextFrameChanged,
            ContextFrameKind, ContextFrameSource, ContextMessageRole,
        };

        let fixture = serde_json::json!({
            "id": "frame-1", "kind": "identity", "source": "runtime_context_update",
            "delivery_status": "accepted", "delivery_channel": "connector_context",
            "message_role": "system",
            "delivery_metadata": {
                "delivery_phase": "stable_system", "delivery_order": 10,
                "cache_policy": "static", "model_channel": "system",
                "agent_consumption": { "target": "", "mode": "consume", "reason": "default_identity_delivery" },
                "frontend_label": "Identity", "connector_profile": { "profile_id": "" }
            },
            "rendered_text": "system prompt", "sections": [], "created_at_ms": 123
        });
        let frame: ContextFrame =
            serde_json::from_value(fixture.clone()).expect("deserialize main-reference payload");
        assert_eq!(frame.kind, ContextFrameKind::Identity);
        assert_eq!(frame.source, ContextFrameSource::RuntimeContextUpdate);
        assert_eq!(frame.delivery_status, ContextDeliveryStatus::Accepted);
        assert_eq!(
            frame.delivery_channel,
            ContextDeliveryChannel::ConnectorContext
        );
        assert_eq!(frame.message_role, ContextMessageRole::System);
        assert_eq!(serde_json::to_value(&frame).unwrap(), fixture);

        let event = BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(Box::new(
            ContextFrameChanged { frame },
        )));
        let value = serde_json::to_value(event).expect("serialize typed context frame event");
        assert_eq!(value["payload"]["kind"], "context_frame_changed");
        assert_eq!(value["payload"]["data"]["frame"], fixture);
    }

    #[test]
    fn owned_context_frame_vocabulary_round_trips_without_claiming_producer_semantics() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/context_frames_canonical_roundtrip.json"
        ))
        .expect("parse wrapper-neutral owned vocabulary fixture");
        assert_eq!(fixture["fixture_kind"], "protocol_roundtrip_only");
        let frames: Vec<ContextFrame> =
            serde_json::from_value(fixture["frames"].clone()).expect("decode owned frames");
        assert_eq!(serde_json::to_value(&frames).unwrap(), fixture["frames"]);
        assert_eq!(
            frames
                .iter()
                .map(|frame| (frame.kind.as_key(), frame.delivery_metadata.delivery_order))
                .collect::<Vec<_>>(),
            vec![
                ("identity", 10),
                ("user_context", 12),
                ("environment", 15),
                ("system_guidelines", 20),
                ("compaction_summary", 30),
                ("assignment_context", 40),
                ("capability_state_delta", 50),
                ("memory_context", 60),
                ("pending_action", 70),
                ("system_delivery", 100),
                ("system_notice", 100),
            ]
        );
        for frame in &frames {
            let expected = ContextDeliveryMetadata::for_frame(
                frame.kind,
                frame.delivery_channel,
                frame.message_role,
            );
            assert_eq!(
                frame.delivery_metadata,
                expected,
                "{} metadata",
                frame.kind.as_key()
            );
        }
    }

    #[test]
    fn wrapper_neutral_normalization_only_removes_coordinates_identity_and_time() {
        fn normalize(mut value: serde_json::Value) -> serde_json::Value {
            let data = &mut value["payload"]["data"];
            data.as_object_mut().unwrap().remove("thread_id");
            data.as_object_mut().unwrap().remove("turn_id");
            let frame = data["frame"].as_object_mut().unwrap();
            frame.remove("id");
            frame.remove("created_at_ms");
            value
        }
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/context_frames_canonical_roundtrip.json"
        ))
        .unwrap();
        let frame = fixture["frames"][0].clone();
        let left = serde_json::json!({"payload":{"data":{"thread_id":"old","turn_id":"old","frame":frame}}});
        let mut right = left.clone();
        right["payload"]["data"]["thread_id"] = serde_json::json!("new");
        right["payload"]["data"]["turn_id"] = serde_json::json!("new");
        right["payload"]["data"]["frame"]["id"] = serde_json::json!("new-id");
        right["payload"]["data"]["frame"]["created_at_ms"] = serde_json::json!(999);
        assert_eq!(normalize(left.clone()), normalize(right.clone()));
        right["payload"]["data"]["frame"]["rendered_text"] = serde_json::json!("changed");
        assert_ne!(normalize(left), normalize(right));
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
