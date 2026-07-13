//! Shared executable behavior checks for runtime and driver implementations.

pub mod session_parity;

#[cfg(all(test, feature = "pinned-main-capture"))]
mod pinned_main_tool_oracle {
    #[path = "../../../../../AgentDash-main-reference/crates/agentdash-executor/src/connectors/pi_agent/session_item_identity.rs"]
    mod session_item_identity;
    #[allow(dead_code)]
    mod stream_mapper {
        include!(concat!(env!("OUT_DIR"), "/pinned_main_stream_mapper.rs"));
    }

    mod codex_notification {
        use std::sync::Arc;

        use agentdash_agent_protocol::generated::codex_v2::server_notification::ThreadNameUpdatedNotification;
        use agentdash_agent_protocol::{
            BackboneEnvelope, BackboneEvent, ItemCompletedNotification, ItemStartedNotification,
            PlatformEvent, SourceInfo, TraceInfo,
        };
        use serde_json::Value;
        use tokio::sync::{Mutex, mpsc};

        #[derive(Debug)]
        struct ConnectorError;

        struct JSONRPCNotification {
            method: String,
            params: Option<Value>,
        }

        macro_rules! diag {
            ($($tokens:tt)*) => {{}};
        }

        const CODEX_SOURCE_TITLE: &str = "codex";

        include!(concat!(
            env!("OUT_DIR"),
            "/pinned_main_codex_notification.rs"
        ));

        pub async fn capture(method: &str, params: Value) -> Option<Value> {
            let (tx, mut rx) = mpsc::channel(1);
            let source = SourceInfo {
                connector_id: "codex-bridge".to_string(),
                connector_type: "local_executor".to_string(),
                executor_id: Some("CODEX".to_string()),
            };
            handle_server_notification(
                JSONRPCNotification {
                    method: method.to_string(),
                    params: Some(params),
                },
                "source-thread",
                &tx,
                &source,
                "source-turn",
                &Arc::new(Mutex::new(None)),
            )
            .await;
            drop(tx);
            rx.recv()
                .await
                .map(|result| serde_json::to_value(result.expect("Main Codex mapping").event))
                .transpose()
                .expect("serialize Main Codex protected body")
        }
    }

    #[allow(dead_code)]
    mod application_terminal {
        use agentdash_agent_protocol::{
            BackboneEnvelope, BackboneEvent, PlatformEvent, RuntimeTerminalDiagnostic, SourceInfo,
            TraceInfo,
        };

        include!(concat!(env!("OUT_DIR"), "/pinned_main_turn_terminal.rs"));

        pub fn capture(
            terminal: &str,
            message: Option<&str>,
            diagnostic: Option<RuntimeTerminalDiagnostic>,
            started_at_ms: Option<i64>,
            completed_at_ms: i64,
        ) -> serde_json::Value {
            let terminal_kind = match terminal {
                "completed" => TurnTerminalKind::Completed,
                "failed" => TurnTerminalKind::Failed,
                "interrupted" => TurnTerminalKind::Interrupted,
                "lost" => TurnTerminalKind::Lost,
                other => panic!("unsupported terminal fixture kind {other}"),
            };
            let timing = started_at_ms
                .map(|started_at_ms| TurnTiming::complete(started_at_ms, completed_at_ms));
            let envelope = build_turn_terminal_envelope_with_timing(
                "session-terminal-0001",
                &SourceInfo {
                    connector_id: "application".into(),
                    connector_type: "runtime".into(),
                    executor_id: None,
                },
                "turn-terminal-0001",
                terminal_kind,
                message.map(str::to_string),
                diagnostic,
                timing,
            );
            serde_json::to_value(envelope.event).expect("Main terminal protected body")
        }
    }

    mod application_input {
        use agentdash_agent_protocol::{
            BackboneEnvelope, BackboneEvent, SourceInfo, TraceInfo, UserInputBlock,
            UserInputSource, UserInputSubmissionKind, UserInputSubmittedNotification,
        };

        include!(concat!(env!("OUT_DIR"), "/pinned_main_input_turn.rs"));
    }

    #[allow(dead_code)]
    mod application_delivery {
        use agentdash_agent_protocol::{
            BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
        };

        include!(concat!(env!("OUT_DIR"), "/pinned_main_delivery.rs"));
    }

    mod terminal_pty {
        include!(concat!(env!("OUT_DIR"), "/pinned_main_terminal_pty.rs"));
    }

    mod session_rewind {
        use agentdash_agent_protocol::{
            BackboneEnvelope, BackboneEvent, PlatformEvent, SessionRewindReason, SessionRewound,
            SourceInfo, TraceInfo,
        };

        include!(concat!(env!("OUT_DIR"), "/pinned_main_session_rewind.rs"));
    }

    mod control_projection {
        use agentdash_agent_protocol::{
            BackboneEnvelope, BackboneEvent, ControlPlaneProjection,
            ControlPlaneProjectionChangeReason, ControlPlaneProjectionChanged,
            ControlPlaneWorkspaceModulePresentation, PlatformEvent, SourceInfo, TraceInfo,
        };

        include!(concat!(
            env!("OUT_DIR"),
            "/pinned_main_control_projection.rs"
        ));
    }

    #[allow(dead_code)]
    mod hook_trace {
        include!(concat!(env!("OUT_DIR"), "/pinned_main_hook_trace.rs"));
    }

    async fn verify_pinned_main_codex_fixture(
        fixture: &serde_json::Value,
    ) -> Result<usize, String> {
        let main_methods = fixture["main_capture_methods"]
            .as_array()
            .ok_or_else(|| "missing Main capture method list".to_string())?
            .iter()
            .map(|method| {
                method
                    .as_str()
                    .ok_or_else(|| "invalid Main method".to_string())
            })
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
        let scenarios = fixture["scenarios"]
            .as_array()
            .ok_or_else(|| "missing Codex scenarios".to_string())?;
        let mut captured = 0;
        for scenario in scenarios {
            let method = scenario["method"]
                .as_str()
                .ok_or_else(|| "invalid Codex method".to_string())?;
            if !main_methods.contains(method) {
                continue;
            }
            let actual = codex_notification::capture(method, scenario["params"].clone())
                .await
                .ok_or_else(|| format!("pinned Main emitted no event for {method}"))?;
            if actual != scenario["event"] {
                return Err(format!("pinned Main Codex event drifted: {method}"));
            }
            captured += 1;
        }
        Ok(captured)
    }

    fn committed_codex_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../agentdash-integration-codex/fixtures/main-presentation.json"
        ))
        .expect("committed Codex Main fixture")
    }

    #[tokio::test]
    async fn pinned_main_codex_notification_mapper_matches_committed_fixture() {
        let captured = verify_pinned_main_codex_fixture(&committed_codex_fixture())
            .await
            .expect("pinned Main Codex protected bodies");
        assert_eq!(captured, 19, "all pinned Main Codex scenarios must execute");
    }

    #[tokio::test]
    async fn pinned_main_codex_capture_rejects_tampered_fixture_body() {
        let mut fixture = committed_codex_fixture();
        let scenario = fixture["scenarios"]
            .as_array_mut()
            .expect("Codex scenarios")
            .iter_mut()
            .find(|scenario| scenario["method"] == "turn/plan/updated")
            .expect("Main plan scenario");
        scenario["event"]["payload"]["explanation"] = serde_json::json!("tampered fixture body");
        let error = verify_pinned_main_codex_fixture(&fixture)
            .await
            .expect_err("tampered Main fixture body must fail");
        assert!(error.contains("turn/plan/updated"), "{error}");
    }

    fn capture_pinned_main_native_events(
        events: Vec<agentdash_agent::AgentEvent>,
    ) -> Vec<serde_json::Value> {
        use std::collections::HashMap;

        use agentdash_agent_protocol::SourceInfo;

        let source = SourceInfo {
            connector_id: "pi-agent".to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: None,
        };
        let identity = session_item_identity::SessionItemIdentity::new();
        let mut entry_index = 0;
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        events
            .iter()
            .flat_map(|event| {
                stream_mapper::convert_event_to_envelopes_with_runtime_context(
                    event,
                    "native-thread",
                    &source,
                    "runtime-turn",
                    stream_mapper::StreamMapperEventState {
                        entry_index: &mut entry_index,
                        chunk_emit_states: &mut chunk_emit_states,
                        tool_call_states: &mut tool_call_states,
                    },
                    stream_mapper::StreamMapperRuntimeContext {
                        model_context_window: Some(200_000),
                        session_identity: Some(identity.clone()),
                        ..Default::default()
                    },
                )
            })
            .map(|envelope| {
                let mut event = serde_json::to_value(envelope.event).expect("Main Native event");
                let payload = event.get_mut("payload").expect("Main Native payload");
                for field in ["startedAtMs", "updatedAtMs", "completedAtMs"] {
                    if payload.get(field).is_some() {
                        payload[field] = serde_json::json!(1_783_684_800_000_i64);
                    }
                }
                if let Some(item) = payload
                    .get_mut("item")
                    .and_then(serde_json::Value::as_object_mut)
                    && item.get("type").and_then(serde_json::Value::as_str) == Some("agentMessage")
                {
                    item.entry("phase").or_insert(serde_json::Value::Null);
                    item.entry("memoryCitation")
                        .or_insert(serde_json::Value::Null);
                }
                if let Some(item) = payload
                    .get_mut("item")
                    .and_then(serde_json::Value::as_object_mut)
                    && item.get("type").and_then(serde_json::Value::as_str)
                        == Some("dynamicToolCall")
                {
                    for field in ["namespace", "contentItems", "durationMs", "success"] {
                        item.entry(field).or_insert(serde_json::Value::Null);
                    }
                }
                event
            })
            .collect()
    }

    fn pinned_main_native_inputs() -> Vec<(&'static str, Vec<agentdash_agent::AgentEvent>)> {
        use agentdash_agent::{
            AgentEvent, AgentMessage, AgentRunError, AgentRunErrorKind, AssistantStreamEvent,
            ContentPart, ProviderAttemptPhase, ProviderAttemptStatus, TokenUsage,
        };

        let partial = AgentMessage::assistant("");
        let args = serde_json::json!({"path":"main://README.md"});
        vec![
            (
                "assistant_message_delta_terminal",
                vec![
                    AgentEvent::MessageStart {
                        message: partial.clone(),
                    },
                    AgentEvent::MessageUpdate {
                        message: partial.clone(),
                        event: AssistantStreamEvent::TextDelta {
                            content_index: 0,
                            text: "answer".into(),
                        },
                    },
                    AgentEvent::MessageEnd {
                        message: AgentMessage::assistant("answer"),
                    },
                ],
            ),
            (
                "reasoning_text_summary_terminal",
                vec![
                    AgentEvent::MessageStart {
                        message: partial.clone(),
                    },
                    AgentEvent::MessageUpdate {
                        message: partial.clone(),
                        event: AssistantStreamEvent::ThinkingDelta {
                            content_index: 0,
                            id: Some("reasoning-1".into()),
                            text: "thought".into(),
                        },
                    },
                    AgentEvent::MessageEnd {
                        message: AgentMessage::Assistant {
                            content: vec![ContentPart::reasoning("thought", None, None)],
                            tool_calls: Vec::new(),
                            stop_reason: None,
                            error_message: None,
                            usage: None,
                            timestamp: Some(1),
                        },
                    },
                ],
            ),
            (
                "item_started_updated_completed",
                vec![
                    AgentEvent::ToolExecutionStart {
                        tool_call_id: "tool-1".into(),
                        tool_name: "read".into(),
                        args: args.clone(),
                    },
                    AgentEvent::ToolExecutionUpdate {
                        tool_call_id: "tool-1".into(),
                        tool_name: "read".into(),
                        args: args.clone(),
                        partial_result: serde_json::json!({
                            "content": [{"type":"text","text":"partial"}],
                            "content_items": [{"type":"inputText","text":"partial"}],
                            "is_error": false,
                            "details": null
                        }),
                    },
                    AgentEvent::ToolExecutionEnd {
                        tool_call_id: "tool-1".into(),
                        tool_name: "read".into(),
                        result: serde_json::json!({
                            "content": [{"type":"text","text":"complete"}],
                            "is_error": false,
                            "details": null
                        }),
                        is_error: false,
                    },
                ],
            ),
            (
                "usage_context",
                vec![
                    AgentEvent::MessageStart {
                        message: partial.clone(),
                    },
                    AgentEvent::MessageEnd {
                        message: AgentMessage::Assistant {
                            content: vec![ContentPart::text("answer")],
                            tool_calls: Vec::new(),
                            stop_reason: None,
                            error_message: None,
                            usage: Some(TokenUsage {
                                input: 10,
                                cache_read_input: 2,
                                cache_creation_input: 3,
                                output: 4,
                            }),
                            timestamp: Some(1),
                        },
                    },
                ],
            ),
            (
                "provider_phases_error",
                [
                    ProviderAttemptPhase::Connecting,
                    ProviderAttemptPhase::ConnectedWaitingFirstDelta,
                    ProviderAttemptPhase::Streaming,
                    ProviderAttemptPhase::RetryScheduled,
                    ProviderAttemptPhase::Retrying,
                    ProviderAttemptPhase::Failed,
                    ProviderAttemptPhase::Succeeded,
                ]
                .into_iter()
                .map(|phase| AgentEvent::ProviderAttemptStatus {
                    status: ProviderAttemptStatus {
                        phase,
                        attempt: 1,
                        max_attempts: 2,
                        will_retry: matches!(
                            phase,
                            ProviderAttemptPhase::RetryScheduled | ProviderAttemptPhase::Retrying
                        ),
                        delay_ms: Some(250),
                        reason_code: Some("rate_limit".into()),
                        message: Some("provider phase".into()),
                        provider: Some("provider".into()),
                        model: Some("model".into()),
                    },
                })
                .chain([AgentEvent::RunError {
                    error: AgentRunError::new(AgentRunErrorKind::Provider, "provider failed"),
                }])
                .collect(),
            ),
            (
                "thread_status_title_compaction",
                vec![AgentEvent::ContextCompactionFailed {
                    item_id: "compaction-1".into(),
                    error: "compaction failed".into(),
                    metadata: None,
                }],
            ),
            (
                "interactions_all_connectors",
                vec![
                    AgentEvent::ToolExecutionPendingApproval {
                        tool_call_id: "approval-1".into(),
                        tool_name: "shell_exec".into(),
                        args: serde_json::json!({"command":"echo ok"}),
                        reason: "permission required".into(),
                        details: Some(serde_json::json!({"scope":"workspace"})),
                    },
                    AgentEvent::ToolExecutionApprovalResolved {
                        tool_call_id: "approval-1".into(),
                        tool_name: "shell_exec".into(),
                        args: serde_json::json!({"command":"echo ok"}),
                        approved: true,
                        reason: Some("approved".into()),
                    },
                ],
            ),
        ]
    }

    fn verify_pinned_main_native_fixture(fixture: &serde_json::Value) -> Result<usize, String> {
        let scenarios = fixture["scenarios"]
            .as_object()
            .ok_or_else(|| "missing Native scenarios".to_string())?;
        let mut captured = 0;
        for (scenario, inputs) in pinned_main_native_inputs() {
            let actual = capture_pinned_main_native_events(inputs);
            let expected = scenarios
                .get(scenario)
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| format!("missing Native fixture scenario {scenario}"))?
                .iter()
                .map(|record| record["event"].clone())
                .collect::<Vec<_>>();
            if actual != expected {
                return Err(format!(
                    "pinned Main Native event drifted: {scenario}\nactual={}\nexpected={}",
                    serde_json::to_string(&actual).expect("serialize actual Native capture"),
                    serde_json::to_string(&expected).expect("serialize expected Native capture")
                ));
            }
            captured += 1;
        }
        Ok(captured)
    }

    fn committed_native_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../agentdash-integration-native-agent/fixtures/main-oracle-presentation.json"
        ))
        .expect("committed Native Main fixture")
    }

    #[test]
    fn pinned_main_native_mapper_matches_all_committed_scenarios() {
        assert_eq!(
            verify_pinned_main_native_fixture(&committed_native_fixture())
                .expect("pinned Main Native protected bodies"),
            7
        );
    }

    #[test]
    fn pinned_main_native_capture_rejects_tampered_fixture_body() {
        let mut fixture = committed_native_fixture();
        fixture["scenarios"]["assistant_message_delta_terminal"][0]["event"]["payload"]["delta"] =
            serde_json::json!("tampered fixture body");
        let error = verify_pinned_main_native_fixture(&fixture)
            .expect_err("tampered Native fixture body must fail");
        assert!(
            error.contains("assistant_message_delta_terminal"),
            "{error}"
        );
    }

    fn committed_terminal_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../agentdash-application-agentrun/tests/fixtures/session-parity/main-957fa9d/turn-terminal.json"
        ))
        .expect("committed terminal Main fixture")
    }

    fn event_value(event: agentdash_agent_protocol::BackboneEvent) -> serde_json::Value {
        serde_json::to_value(event).expect("serialize pinned Main protected body")
    }

    fn source(
        connector_id: &str,
        connector_type: &str,
        executor_id: Option<&str>,
    ) -> agentdash_agent_protocol::SourceInfo {
        agentdash_agent_protocol::SourceInfo {
            connector_id: connector_id.into(),
            connector_type: connector_type.into(),
            executor_id: executor_id.map(str::to_string),
        }
    }

    fn committed_user_submit_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../fixtures/session-parity/main/user-submit.json"
        ))
        .expect("committed user submit Main fixture")
    }

    fn committed_input_steer_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../agentdash-application-agentrun/tests/fixtures/session-parity/main-957fa9d/input-steer.json"
        ))
        .expect("committed input steer Main fixture")
    }

    fn committed_input_modalities_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../agentdash-application-agentrun/tests/fixtures/session-parity/main-957fa9d/input-modalities.json"
        ))
        .expect("committed input modalities Main fixture")
    }

    fn committed_delivery_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../agentdash-application-agentrun/tests/fixtures/session-parity/main-957fa9d/delivery-sources.json"
        ))
        .expect("committed delivery Main fixture")
    }

    #[test]
    fn pinned_main_input_prompt_and_turn_started_match_committed_fixture() {
        use agentdash_agent_protocol::{UserInputBlock, UserInputSource, UserInputSubmissionKind};

        let fixture = committed_user_submit_fixture();
        let input = vec![UserInputBlock::Text {
            text: "hello".into(),
            text_elements: Vec::new(),
        }];
        let submitted = application_input::build_user_input_submitted_envelope(
            "session-main-0001",
            &source("fixture-connector", "native", None),
            "turn-main-0001",
            "turn-main-0001:user-input:0",
            UserInputSubmissionKind::Prompt,
            UserInputSource::core_composer(),
            input,
        );
        let started = application_input::build_turn_started_envelope(
            "session-main-0001",
            &source("fixture-connector", "native", None),
            "turn-main-0001",
            1_783_684_800_001,
        );
        assert_eq!(
            event_value(submitted.event),
            fixture["frames"][0]["notification"]["event"]
        );
        assert_eq!(
            event_value(started.event),
            fixture["frames"][1]["notification"]["event"]
        );
    }

    #[test]
    fn pinned_main_input_steer_matches_committed_fixture() {
        use agentdash_agent_protocol::{UserInputBlock, UserInputSource, UserInputSubmissionKind};

        let fixture = committed_input_steer_fixture();
        let submitted = application_input::build_user_input_submitted_envelope(
            "session-main-0001",
            &source("agent_run_mailbox", "platform", Some("AGENT_RUN_MAILBOX")),
            "turn-main-0001",
            fixture["provenance"]["item_id"]
                .as_str()
                .expect("steer item id"),
            UserInputSubmissionKind::Steer,
            UserInputSource::core_composer(),
            vec![UserInputBlock::Text {
                text: "steer now".into(),
                text_elements: Vec::new(),
            }],
        );
        assert_eq!(
            event_value(submitted.event),
            fixture["frames"][0]["notification"]["event"]
        );
    }

    #[test]
    fn pinned_main_input_modalities_and_turn_started_match_committed_fixture() {
        use agentdash_agent_protocol::{UserInputBlock, UserInputSource, UserInputSubmissionKind};

        let fixture = committed_input_modalities_fixture();
        let input: Vec<UserInputBlock> =
            serde_json::from_value(fixture["protected_events"][0]["payload"]["content"].clone())
                .expect("Main modality input blocks");
        let submitted = application_input::build_user_input_submitted_envelope(
            "session-modalities-0001",
            &source("fixture-connector", "native", None),
            "turn-modalities-0001",
            "turn-modalities-0001:user-input:0",
            UserInputSubmissionKind::Prompt,
            UserInputSource::core_composer(),
            input,
        );
        let started = application_input::build_turn_started_envelope(
            "session-modalities-0001",
            &source("fixture-connector", "native", None),
            "turn-modalities-0001",
            1_783_684_800_000,
        );
        assert_eq!(event_value(submitted.event), fixture["protected_events"][0]);
        assert_eq!(event_value(started.event), fixture["protected_events"][1]);
    }

    #[test]
    fn pinned_main_system_workflow_companion_delivery_matches_committed_fixture() {
        use agentdash_agent_protocol::{UserInputBlock, UserInputSource, UserInputSubmissionKind};

        let fixture = committed_delivery_fixture();
        for (case, turn_id, message) in [
            ("system", "turn-system-0001", "system wake"),
            ("workflow", "turn-workflow-0001", "workflow continue"),
            ("routine", "turn-routine-0001", "routine wake"),
            (
                "companion_marker",
                "turn-companion-marker-0001",
                "<subagent_notification>{\"status\":\"completed\"}</subagent_notification>",
            ),
        ] {
            assert_eq!(
                event_value(application_delivery::capture(
                    match case {
                        "companion_marker" => "session-companion-marker-0001",
                        _ => "session-delivery-0001",
                    },
                    turn_id,
                    case,
                    message,
                )),
                fixture["cases"][case]["first_event"],
                "delivery case {case}"
            );
        }

        let companion = application_input::build_user_input_submitted_envelope(
            "session-companion-0001",
            &source("fixture-connector", "native", None),
            "turn-companion-0001",
            "turn-companion-0001:user-input:0",
            UserInputSubmissionKind::Prompt,
            UserInputSource::new("companion", "dispatch", "agent").with_route("sub"),
            vec![UserInputBlock::Text {
                text: "companion dispatch".into(),
                text_elements: Vec::new(),
            }],
        );
        assert_eq!(
            event_value(companion.event),
            fixture["cases"]["companion"]["first_event"]
        );
    }

    fn verify_pinned_main_terminal_fixture(fixture: &serde_json::Value) -> Result<usize, String> {
        use agentdash_agent_protocol::RuntimeTerminalDiagnostic;

        let cases = [
            (
                "completed_with_timing",
                application_terminal::capture(
                    "completed",
                    None,
                    None,
                    Some(1_783_684_800_000),
                    1_783_684_801_250,
                ),
            ),
            (
                "completed_without_timing",
                application_terminal::capture("completed", None, None, None, 1_783_684_801_250),
            ),
            (
                "failed_with_diagnostic",
                application_terminal::capture(
                    "failed",
                    Some("provider failed"),
                    Some(RuntimeTerminalDiagnostic {
                        kind: "provider".into(),
                        code: Some("rate_limit".into()),
                        http_status: Some(429),
                        provider: Some("openai".into()),
                        model: Some("gpt-5".into()),
                        message: "rate limited".into(),
                        retryable: true,
                    }),
                    Some(1_783_684_800_000),
                    1_783_684_801_250,
                ),
            ),
            (
                "failed_rewind",
                serde_json::to_value(session_rewind::capture(
                    "turn-terminal-0001",
                    None,
                    None,
                    None,
                    "runtime_failure",
                    Some("provider failed".into()),
                ))
                .expect("serialize pinned Main rewind body"),
            ),
            (
                "interrupted_without_timing",
                application_terminal::capture(
                    "interrupted",
                    Some("cancelled"),
                    None,
                    None,
                    1_783_684_801_250,
                ),
            ),
            (
                "interrupted_rewind",
                serde_json::to_value(session_rewind::capture(
                    "turn-terminal-0001",
                    None,
                    None,
                    None,
                    "runtime_interrupted",
                    Some("cancelled".into()),
                ))
                .expect("serialize pinned Main interrupted rewind body"),
            ),
            (
                "lost_without_timing",
                application_terminal::capture(
                    "lost",
                    Some("driver lost"),
                    None,
                    None,
                    1_783_684_801_250,
                ),
            ),
            (
                "lost_rewind",
                serde_json::to_value(session_rewind::capture(
                    "turn-terminal-0001",
                    None,
                    None,
                    None,
                    "runtime_lost",
                    Some("driver lost".into()),
                ))
                .expect("serialize pinned Main lost rewind body"),
            ),
            (
                "rewind_stable_provider_retry",
                serde_json::to_value(session_rewind::capture(
                    "turn-discarded",
                    Some(3),
                    Some(42),
                    Some("turn-stable"),
                    "provider_retry",
                    Some(" retry after provider failure ".into()),
                ))
                .expect("serialize pinned Main stable rewind body"),
            ),
            (
                "rewind_stable_runtime_failure",
                serde_json::to_value(session_rewind::capture(
                    "turn-discarded",
                    Some(3),
                    Some(42),
                    Some("turn-stable"),
                    "runtime_failure",
                    Some(" retry after provider failure ".into()),
                ))
                .expect("serialize pinned Main stable Runtime rewind body"),
            ),
            (
                "rewind_html_provider_failure",
                serde_json::to_value(session_rewind::capture(
                    "turn-discarded",
                    None,
                    None,
                    None,
                    "provider_failure",
                    Some("upstream failed: <html>private body</html>".into()),
                ))
                .expect("serialize pinned Main HTML rewind body"),
            ),
            (
                "rewind_html_runtime_failure",
                serde_json::to_value(session_rewind::capture(
                    "turn-discarded",
                    None,
                    None,
                    None,
                    "runtime_failure",
                    Some("upstream failed: <html>private body</html>".into()),
                ))
                .expect("serialize pinned Main HTML Runtime rewind body"),
            ),
            (
                "rewind_long_runtime_failure",
                serde_json::to_value(session_rewind::capture(
                    "turn-discarded",
                    None,
                    None,
                    None,
                    "unknown",
                    Some("x".repeat(600)),
                ))
                .expect("serialize pinned Main bounded rewind body"),
            ),
        ];
        let count = cases.len();
        for (case, actual) in cases {
            if actual != fixture["cases"][case] {
                return Err(format!("pinned Main terminal event drifted: {case}"));
            }
        }
        Ok(count)
    }

    #[test]
    fn pinned_main_terminal_builder_matches_committed_meta_cases() {
        assert_eq!(
            verify_pinned_main_terminal_fixture(&committed_terminal_fixture())
                .expect("pinned Main terminal protected bodies"),
            13
        );
    }

    #[test]
    fn pinned_main_session_rewind_builder_preserves_boundaries_reason_and_bounded_messages() {
        let stable = serde_json::to_value(session_rewind::capture(
            "turn-discarded",
            Some(3),
            Some(42),
            Some("turn-stable"),
            "provider_retry",
            Some(" retry after provider failure ".into()),
        ))
        .expect("serialize pinned Main stable rewind");
        assert_eq!(stable["payload"]["data"]["discarded_entry_index"], 3);
        assert_eq!(stable["payload"]["data"]["stable_event_seq"], 42);
        assert_eq!(stable["payload"]["data"]["stable_turn_id"], "turn-stable");
        assert_eq!(stable["payload"]["data"]["reason"], "provider_retry");
        assert_eq!(
            stable["payload"]["data"]["message"],
            "retry after provider failure"
        );

        let html = serde_json::to_value(session_rewind::capture(
            "turn-discarded",
            None,
            None,
            None,
            "provider_failure",
            Some("upstream failed: <html>private body</html>".into()),
        ))
        .expect("serialize pinned Main HTML rewind");
        assert_eq!(html["payload"]["data"]["reason"], "provider_failure");
        assert_eq!(
            html["payload"]["data"]["message"],
            "upstream failed; HTML error response body omitted"
        );

        let long = serde_json::to_value(session_rewind::capture(
            "turn-discarded",
            None,
            None,
            None,
            "unknown",
            Some("x".repeat(600)),
        ))
        .expect("serialize pinned Main bounded rewind");
        let message = long["payload"]["data"]["message"]
            .as_str()
            .expect("bounded rewind message");
        assert_eq!(message.chars().count(), 515);
        assert!(message.ends_with("..."));
        assert_eq!(long["payload"]["data"]["reason"], "runtime_failure");
    }

    #[test]
    fn pinned_main_terminal_capture_rejects_tampered_fixture_body() {
        let mut fixture = committed_terminal_fixture();
        fixture["cases"]["completed_with_timing"]["payload"]["data"]["value"]["terminal_type"] =
            serde_json::json!("tampered");
        let error = verify_pinned_main_terminal_fixture(&fixture)
            .expect_err("tampered terminal fixture must fail");
        assert!(error.contains("completed_with_timing"), "{error}");
    }

    fn committed_platform_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../fixtures/session-parity/main/platform-events.json"
        ))
        .expect("committed platform Main fixture")
    }

    fn verify_pinned_main_terminal_pty_fixture(
        fixture: &serde_json::Value,
    ) -> Result<usize, String> {
        let terminal =
            terminal_pty::capture_terminal_output(&agentdash_relay::TerminalOutputPayload {
                terminal_id: "terminal-1".into(),
                data: "partial".into(),
                truncation: agentdash_relay::ToolShellTruncationInfo {
                    truncated: true,
                    omitted_bytes: 17,
                    omitted_chunks: 1,
                    omitted_tokens_estimate: None,
                },
            });
        let pty =
            terminal_pty::capture_pty_state(&agentdash_relay::PtyTerminalStateChangedPayload {
                terminal_id: "terminal-1".into(),
                state: agentdash_relay::PtyTerminalProcessState::Lost,
                exit_code: None,
                message: Some("backend disconnected".into()),
            });
        for (case, actual) in [
            ("terminal_output_truncated", terminal),
            ("pty_lost_with_message", pty),
        ] {
            let actual = serde_json::to_value(actual)
                .map_err(|error| format!("serialize {case}: {error}"))?;
            if actual != fixture["cases"][case] {
                return Err(format!("pinned Main terminal/PTY event drifted: {case}"));
            }
        }
        Ok(2)
    }

    #[test]
    fn pinned_main_terminal_pty_projections_match_committed_fixture() {
        assert_eq!(
            verify_pinned_main_terminal_pty_fixture(&committed_platform_fixture())
                .expect("pinned Main terminal/PTY protected bodies"),
            2
        );
    }

    #[test]
    fn pinned_main_terminal_pty_capture_rejects_tampered_fixture_body() {
        let mut fixture = committed_platform_fixture();
        fixture["cases"]["terminal_output_truncated"]["payload"]["data"]["data"] =
            serde_json::json!("tampered");
        let error = verify_pinned_main_terminal_pty_fixture(&fixture)
            .expect_err("tampered terminal/PTY fixture must fail");
        assert!(error.contains("terminal_output_truncated"), "{error}");
    }

    fn verify_pinned_main_control_projection_fixture(
        fixture: &serde_json::Value,
    ) -> Result<(), String> {
        let actual = serde_json::to_value(control_projection::capture(serde_json::json!({
            "blocks": [{"kind": "text", "text": "ready"}],
            "revision": 7
        })))
        .map_err(|error| format!("serialize workspace module control projection: {error}"))?;
        if actual != fixture["cases"]["workspace_module_presented"] {
            return Err("pinned Main workspace module control projection drifted".into());
        }
        Ok(())
    }

    #[test]
    fn pinned_main_workspace_module_control_projection_matches_committed_fixture() {
        verify_pinned_main_control_projection_fixture(&committed_platform_fixture())
            .expect("pinned Main workspace module control protected body");
    }

    #[test]
    fn pinned_main_workspace_module_control_projection_rejects_tampered_fixture_body() {
        let mut fixture = committed_platform_fixture();
        fixture["cases"]["workspace_module_presented"]["payload"]["data"]["run_id"] =
            serde_json::json!("tampered");
        let error = verify_pinned_main_control_projection_fixture(&fixture)
            .expect_err("tampered control projection fixture body must fail");
        assert!(error.contains("control projection"), "{error}");
    }

    fn pinned_main_hook_entry(
        trigger: agentdash_spi::HookTraceTrigger,
        decision: &str,
    ) -> agentdash_spi::HookTraceEntry {
        agentdash_spi::HookTraceEntry {
            sequence: 7,
            timestamp_ms: 1_783_684_800_000,
            revision: 3,
            trigger,
            decision: decision.into(),
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            matched_rule_keys: Vec::new(),
            refresh_snapshot: false,
            effects_applied: false,
            block_reason: None,
            completion: None,
            diagnostics: Vec::new(),
            injections: Vec::new(),
        }
    }

    fn capture_pinned_main_hook_cases() -> serde_json::Map<String, serde_json::Value> {
        use agentdash_agent_protocol::SourceInfo;
        use agentdash_spi::{HookDiagnosticEntry, HookTraceTrigger};

        let mut deny = pinned_main_hook_entry(HookTraceTrigger::BeforeTool, "deny");
        deny.tool_name = Some("shell".into());
        deny.tool_call_id = Some("call-hook-1".into());
        deny.matched_rule_keys = vec!["policy:deny-shell".into()];
        deny.block_reason = Some("shell denied".into());
        deny.diagnostics = vec![HookDiagnosticEntry {
            code: "tool_denied".into(),
            message: "shell denied by policy".into(),
        }];

        let ask = pinned_main_hook_entry(HookTraceTrigger::BeforeTool, "ask");
        let rewrite = pinned_main_hook_entry(HookTraceTrigger::BeforeTool, "rewrite");
        let mut allow = pinned_main_hook_entry(HookTraceTrigger::BeforeTool, "allow");
        allow.matched_rule_keys = vec!["policy:observed".into()];
        let mut effects = pinned_main_hook_entry(HookTraceTrigger::AfterTool, "effects_applied");
        effects.effects_applied = true;
        effects.matched_rule_keys = vec!["workflow:tool-effect".into()];
        let noop = pinned_main_hook_entry(HookTraceTrigger::AfterTool, "noop");

        let source = || SourceInfo {
            connector_id: "pi-agent".into(),
            connector_type: "local_executor".into(),
            executor_id: Some("PI_AGENT".into()),
        };
        [
            ("hook_before_tool_deny", deny),
            ("hook_before_tool_ask", ask),
            ("hook_before_tool_rewrite", rewrite),
            ("hook_before_tool_allow_ephemeral", allow),
            ("hook_after_tool_effects", effects),
            ("hook_after_tool_noop_dropped", noop),
        ]
        .into_iter()
        .map(|(case, entry)| {
            let body = hook_trace::build_hook_trace_envelope(
                "session-hook-0001",
                Some("turn-hook-0001"),
                source(),
                &entry,
            )
            .map(|envelope| serde_json::to_value(envelope.event).expect("Main hook protected body"))
            .unwrap_or(serde_json::Value::Null);
            (case.to_string(), body)
        })
        .collect()
    }

    fn verify_pinned_main_hook_fixture(fixture: &serde_json::Value) -> Result<(), String> {
        let actual = capture_pinned_main_hook_cases();
        for (case, body) in &actual {
            if *body != fixture["cases"][case] {
                return Err(format!(
                    "pinned Main hook projection drifted: {case}\nactual={}",
                    serde_json::to_string(&actual).expect("serialize actual Main hook cases")
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn pinned_main_hook_trace_projection_matches_committed_fixture() {
        verify_pinned_main_hook_fixture(&committed_platform_fixture())
            .expect("pinned Main hook protected bodies");
    }

    #[test]
    fn pinned_main_hook_trace_projection_rejects_tampered_fixture_body() {
        let mut fixture = committed_platform_fixture();
        fixture["cases"]["hook_before_tool_deny"]["payload"]["data"]["decision"] =
            serde_json::json!("tampered");
        let error = verify_pinned_main_hook_fixture(&fixture)
            .expect_err("tampered hook fixture body must fail");
        assert!(error.contains("hook_before_tool_deny"), "{error}");
    }

    #[test]
    fn pinned_main_mcp_dynamic_mapper_matches_committed_fixture() {
        use std::collections::HashMap;

        use agentdash_agent::{AgentEvent, AgentToolResult, ContentPart};
        use agentdash_agent_protocol::SourceInfo;

        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../agentdash-agent-runtime/fixtures/main-mcp-tool-lifecycle.json"
        ))
        .expect("committed MCP parity fixture");
        let source = SourceInfo {
            connector_id: "pi-agent".to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: None,
        };

        for scenario in fixture["scenarios"].as_array().expect("MCP scenarios") {
            let id = scenario["id"].as_str().expect("scenario id");
            let tool_name = scenario["runtime_name"].as_str().expect("runtime name");
            let args = scenario["arguments"].clone();
            let progress = scenario["progress_message"]
                .as_str()
                .expect("progress message");
            let completed = scenario["completed_output"]["content_items"][0]["text"]
                .as_str()
                .expect("completed output");
            let failed = scenario["failed_output"]["content_items"][0]["text"]
                .as_str()
                .expect("failed output");
            let tool_call_id = format!("{id}-item");
            let failed_tool_call_id = format!("{id}-item-failed");
            let result = |text: &str, is_error: bool| {
                serde_json::to_value(AgentToolResult {
                    content: vec![ContentPart::text(text)],
                    is_error,
                    details: None,
                })
                .expect("serialize MCP tool result")
            };
            let events = [
                AgentEvent::ToolExecutionStart {
                    tool_call_id: tool_call_id.clone(),
                    tool_name: tool_name.to_string(),
                    args: args.clone(),
                },
                AgentEvent::ToolExecutionUpdate {
                    tool_call_id: tool_call_id.clone(),
                    tool_name: tool_name.to_string(),
                    args: args.clone(),
                    partial_result: result(progress, false),
                },
                AgentEvent::ToolExecutionEnd {
                    tool_call_id: tool_call_id.clone(),
                    tool_name: tool_name.to_string(),
                    result: result(completed, false),
                    is_error: false,
                },
            ];
            let mut entry_index = 0;
            let mut chunk_emit_states = HashMap::new();
            let mut tool_call_states = HashMap::new();
            let mut actual = events
                .iter()
                .flat_map(|event| {
                    stream_mapper::convert_event_to_envelopes_with_runtime_context(
                        event,
                        "session-fixture",
                        &source,
                        "turn-fixture",
                        stream_mapper::StreamMapperEventState {
                            entry_index: &mut entry_index,
                            chunk_emit_states: &mut chunk_emit_states,
                            tool_call_states: &mut tool_call_states,
                        },
                        stream_mapper::StreamMapperRuntimeContext::default(),
                    )
                })
                .map(|envelope| serde_json::to_value(envelope.event).expect("Main MCP event"))
                .collect::<Vec<_>>();

            let failed_start = AgentEvent::ToolExecutionStart {
                tool_call_id: failed_tool_call_id.clone(),
                tool_name: tool_name.to_string(),
                args,
            };
            let _ = stream_mapper::convert_event_to_envelopes_with_runtime_context(
                &failed_start,
                "session-fixture",
                &source,
                "turn-fixture",
                stream_mapper::StreamMapperEventState {
                    entry_index: &mut entry_index,
                    chunk_emit_states: &mut chunk_emit_states,
                    tool_call_states: &mut tool_call_states,
                },
                stream_mapper::StreamMapperRuntimeContext::default(),
            );
            let failed_end = AgentEvent::ToolExecutionEnd {
                tool_call_id: failed_tool_call_id,
                tool_name: tool_name.to_string(),
                result: result(failed, true),
                is_error: true,
            };
            actual.extend(
                stream_mapper::convert_event_to_envelopes_with_runtime_context(
                    &failed_end,
                    "session-fixture",
                    &source,
                    "turn-fixture",
                    stream_mapper::StreamMapperEventState {
                        entry_index: &mut entry_index,
                        chunk_emit_states: &mut chunk_emit_states,
                        tool_call_states: &mut tool_call_states,
                    },
                    stream_mapper::StreamMapperRuntimeContext::default(),
                )
                .into_iter()
                .map(|envelope| serde_json::to_value(envelope.event).expect("Main MCP failure")),
            );

            for (index, event) in actual.iter_mut().enumerate() {
                let payload = event.get_mut("payload").expect("MCP payload");
                let timestamp = 1_720_000_000_000_i64 + i64::try_from(index).unwrap();
                for field in ["startedAtMs", "updatedAtMs", "completedAtMs"] {
                    if payload.get(field).is_some() {
                        payload[field] = serde_json::json!(timestamp);
                    }
                }
            }
            assert_eq!(
                actual,
                scenario["protected_events"]
                    .as_array()
                    .expect("protected MCP events")
                    .clone(),
                "pinned Main MCP protected bodies drifted for {id}"
            );
        }
    }

    #[test]
    fn capture_production_tool_projection_fixture() {
        use std::collections::HashMap;
        use std::io::Write;

        use agentdash_agent::{
            AgentEvent, AgentToolResult, ContentPart, ToolResultAddressProvider,
        };
        use agentdash_agent_protocol::SourceInfo;
        use base64::Engine;
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use sha2::{Digest, Sha256};

        const ORACLE_COMMIT: &str = "957fa9d60";
        const ORACLE_SOURCE_PATH: &str =
            "crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs";
        const ORACLE_SOURCE_BLOB: &str = "ec0cfdf485a231090ba538c546c9c681d6ece1fb";
        const ORACLE_TEST_SOURCE_PATH: &str =
            "crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs";
        const ORACLE_TEST_SOURCE_BLOB: &str = "2e0edfdf25aba4dda36d1d6100fa09fa4475ba99";
        const ORACLE_SOURCE: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../AgentDash-main-reference/crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs"
        ));
        const ORACLE_TEST_SOURCE: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../AgentDash-main-reference/crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs"
        ));
        const CASES: [(&str, &str); 17] = [
            ("main_tool_shell_exec_lifecycle", "shell_exec"),
            ("main_tool_fs_apply_patch_lifecycle", "fs_apply_patch"),
            ("main_tool_fs_read_lifecycle", "fs_read"),
            ("main_tool_fs_grep_lifecycle", "fs_grep"),
            ("main_tool_fs_glob_lifecycle", "fs_glob"),
            ("main_tool_vfs_mounts_dynamic_lifecycle", "mounts_list"),
            (
                "main_tool_complete_lifecycle_node_dynamic_lifecycle",
                "complete_lifecycle_node",
            ),
            (
                "main_tool_companion_request_dynamic_lifecycle",
                "companion_request",
            ),
            (
                "main_tool_companion_respond_dynamic_lifecycle",
                "companion_respond",
            ),
            ("main_tool_task_read_dynamic_lifecycle", "task_read"),
            ("main_tool_task_write_dynamic_lifecycle", "task_write"),
            ("main_tool_wait_activity_dynamic_lifecycle", "wait"),
            (
                "main_tool_workspace_module_list_dynamic_lifecycle",
                "workspace_module_list",
            ),
            (
                "main_tool_workspace_module_describe_dynamic_lifecycle",
                "workspace_module_describe",
            ),
            (
                "main_tool_workspace_module_operate_dynamic_lifecycle",
                "workspace_module_operate",
            ),
            (
                "main_tool_workspace_module_unavailable_dynamic_lifecycle",
                "workspace_module_invoke",
            ),
            (
                "main_tool_workspace_module_present_dynamic_lifecycle",
                "workspace_module_present",
            ),
        ];

        let source = SourceInfo {
            connector_id: "pi-agent".to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: None,
        };
        let scenarios = CASES
            .into_iter()
            .map(|(fixture_id, tool_name)| {
                let args = fixture_args(tool_name);
                let identity = session_item_identity::SessionItemIdentity::new();
                let stable_item_id = identity
                    .tool_result_ref(
                        "turn-fixture",
                        &format!("call-{tool_name}"),
                        tool_name,
                    )
                    .item_id;
                let mut details = serde_json::json!({
                    "readable_ref": {
                        "item_id": stable_item_id,
                        "turn_alias": "turn-fixture",
                        "body_alias": tool_name,
                        "body_kind": "tool_result",
                        "lifecycle_path": format!("lifecycle://session/tool-results/turn-fixture/{tool_name}/result.txt")
                    }
                });
                if tool_name == "shell_exec" {
                    details["type"] = serde_json::json!("shell_output");
                }
                let result = serde_json::to_value(AgentToolResult {
                    content: vec![ContentPart::text(format!("{tool_name} fixture result"))],
                    is_error: false,
                    details: Some(details),
                })
                .expect("serialize fixture result");
                let events = [
                    AgentEvent::ToolExecutionStart {
                        tool_call_id: format!("call-{tool_name}"),
                        tool_name: tool_name.to_string(),
                        args: args.clone(),
                    },
                    AgentEvent::ToolExecutionUpdate {
                        tool_call_id: format!("call-{tool_name}"),
                        tool_name: tool_name.to_string(),
                        args: args.clone(),
                        partial_result: result.clone(),
                    },
                    AgentEvent::ToolExecutionEnd {
                        tool_call_id: format!("call-{tool_name}"),
                        tool_name: tool_name.to_string(),
                        result,
                        is_error: false,
                    },
                ];
                let mut entry_index = 0;
                let mut chunk_emit_states = HashMap::new();
                let mut tool_call_states = HashMap::new();
                let mut protected_events = events
                    .iter()
                    .flat_map(|event| {
                        stream_mapper::convert_event_to_envelopes_with_runtime_context(
                            event,
                            "session-fixture",
                            &source,
                            "turn-fixture",
                            stream_mapper::StreamMapperEventState {
                                entry_index: &mut entry_index,
                                chunk_emit_states: &mut chunk_emit_states,
                                tool_call_states: &mut tool_call_states,
                            },
                            stream_mapper::StreamMapperRuntimeContext {
                                session_identity: Some(identity.clone()),
                                ..Default::default()
                            },
                        )
                    })
                    .map(|envelope| serde_json::to_value(envelope.event).unwrap())
                    .collect::<Vec<_>>();
                for (index, event) in protected_events.iter_mut().enumerate() {
                    let timestamp = 1_720_000_000_000_i64 + i64::try_from(index).unwrap();
                    let payload = event.get_mut("payload").unwrap();
                    for field in ["startedAtMs", "updatedAtMs", "completedAtMs"] {
                        if payload.get(field).is_some() {
                            payload[field] = serde_json::json!(timestamp);
                        }
                    }
                }
                serde_json::json!({
                    "fixture_id": fixture_id,
                    "tool_name": tool_name,
                    "arguments": args,
                    "protected_events": protected_events,
                })
            })
            .collect::<Vec<_>>();
        let capture_bytes = serde_json::to_vec(&scenarios).expect("serialize oracle scenarios");
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&capture_bytes).unwrap();
        let compressed = encoder.finish().unwrap();
        let fixture = serde_json::json!({
            "oracle_commit": ORACLE_COMMIT,
            "oracle_source_path": ORACLE_SOURCE_PATH,
            "oracle_source_sha256": format!("{:x}", Sha256::digest(ORACLE_SOURCE)),
            "oracle_source_blob": ORACLE_SOURCE_BLOB,
            "oracle_test_source": ORACLE_TEST_SOURCE_PATH,
            "oracle_test_source_sha256": format!("{:x}", Sha256::digest(ORACLE_TEST_SOURCE)),
            "oracle_test_source_blob": ORACLE_TEST_SOURCE_BLOB,
            "capture_harness_path": "crates/agentdash-agent-runtime-test-support/src/lib.rs",
            "capture_test_function": "capture_production_tool_projection_fixture",
            "capture_build_path": "crates/agentdash-agent-runtime-test-support/build.rs",
            "capture_method": "The pinned-main-capture feature includes the fixed Main stream mapper, applies three exact-count typed compatibility transforms in build.rs, executes ToolExecutionStart/Update/End, hashes the uncompressed protected scenario JSON, then stores it as gzip+base64.",
            "compatibility_transforms": [
                "ErrorNotification.codex_error_info Option wrapping",
                "TurnError.additional_details Option wrapping",
                "CodexErrorInfo.http_status_code Option wrapping",
                "deterministic capture clock for notification timestamps"
            ],
            "capture_sha256": format!("{:x}", Sha256::digest(&capture_bytes)),
            "encoding": "gzip+base64",
            "protected_scenarios": base64::engine::general_purpose::STANDARD.encode(compressed),
        });
        let committed: serde_json::Value = serde_json::from_str(include_str!(
            "../fixtures/session-parity/main/tool-contributions.json"
        ))
        .expect("committed tool contribution fixture");
        assert_eq!(committed, fixture, "pinned main capture drifted");
    }

    fn fixture_args(tool_name: &str) -> serde_json::Value {
        match tool_name {
            "shell_exec" => serde_json::json!({ "command": "cargo test", "cwd": "platform://" }),
            "fs_apply_patch" => serde_json::json!({
                "patch": "*** Begin Patch\n*** Update File: README.md\n@@\n-old\n+new\n*** End Patch\n"
            }),
            "fs_read" => serde_json::json!({ "path": "README.md", "offset": 4, "limit": 12 }),
            "fs_grep" => {
                serde_json::json!({ "pattern": "AgentDash", "path": "crates", "glob": "*.rs" })
            }
            "fs_glob" => {
                serde_json::json!({ "pattern": "**/*.rs", "path": "crates", "max_results": 50 })
            }
            _ => serde_json::json!({ "fixture": tool_name }),
        }
    }
}

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use agentdash_agent_runtime_contract::{
    AgentRuntimeDriver, DriverCommandEnvelope, DriverDispatchReceipt, DriverError, DriverEventSink,
    RuntimeEvent, RuntimeEventEnvelope, RuntimeInteractionId, RuntimeItemId, RuntimeOperationId,
    RuntimeThreadId, RuntimeTurnId,
};
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConformanceViolation {
    #[error("operation {0} reached terminal without acceptance")]
    OperationTerminalWithoutAcceptance(RuntimeOperationId),
    #[error("operation {0} has more than one acceptance")]
    DuplicateOperationAcceptance(RuntimeOperationId),
    #[error("operation {0} has more than one terminal")]
    DuplicateOperationTerminal(RuntimeOperationId),
    #[error("turn {0} has more than one start or terminal")]
    InvalidTurnTransition(RuntimeTurnId),
    #[error("item {0} has more than one start or terminal")]
    InvalidItemTransition(RuntimeItemId),
    #[error("item {0} received a delta after terminal")]
    DeltaAfterItemTerminal(RuntimeItemId),
    #[error("interaction {0} has more than one request or terminal")]
    InvalidInteractionTransition(RuntimeInteractionId),
    #[error("operation {0} changed its parent thread coordinate")]
    OperationThreadMismatch(RuntimeOperationId),
    #[error("turn {0} changed its parent thread coordinate")]
    TurnThreadMismatch(RuntimeTurnId),
    #[error("item {0} changed its parent thread or turn coordinate")]
    ItemParentMismatch(RuntimeItemId),
    #[error("interaction {0} changed its parent thread or turn coordinate")]
    InteractionParentMismatch(RuntimeInteractionId),
    #[error("trace ended with non-terminal operations, turns, items, or interactions")]
    MissingTerminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HarnessError {
    #[error("driver did not return DriverError::Unsupported")]
    DidNotReturnUnsupported,
    #[error("unsupported command produced a side effect")]
    UnsupportedProducedSideEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Active,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScopedPhase {
    phase: Phase,
    thread_id: RuntimeThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TurnScopedPhase {
    phase: Phase,
    thread_id: RuntimeThreadId,
    turn_id: RuntimeTurnId,
}

#[derive(Debug, Default)]
pub struct RuntimeTraceValidator {
    operations: BTreeMap<RuntimeOperationId, ScopedPhase>,
    turns: BTreeMap<RuntimeTurnId, ScopedPhase>,
    items: BTreeMap<RuntimeItemId, TurnScopedPhase>,
    interactions: BTreeMap<RuntimeInteractionId, TurnScopedPhase>,
}

impl RuntimeTraceValidator {
    pub fn observe(&mut self, envelope: &RuntimeEventEnvelope) -> Result<(), ConformanceViolation> {
        match &envelope.event {
            RuntimeEvent::OperationAccepted { operation_id } => {
                if self
                    .operations
                    .insert(
                        operation_id.clone(),
                        ScopedPhase {
                            phase: Phase::Active,
                            thread_id: envelope.thread_id.clone(),
                        },
                    )
                    .is_some()
                {
                    return Err(ConformanceViolation::DuplicateOperationAcceptance(
                        operation_id.clone(),
                    ));
                }
            }
            RuntimeEvent::OperationTerminal { operation_id, .. } => {
                terminal_scoped(
                    &mut self.operations,
                    operation_id,
                    &envelope.thread_id,
                    ConformanceViolation::OperationTerminalWithoutAcceptance(operation_id.clone()),
                    ConformanceViolation::DuplicateOperationTerminal(operation_id.clone()),
                    ConformanceViolation::OperationThreadMismatch(operation_id.clone()),
                )?;
            }
            RuntimeEvent::TurnStarted { turn_id, .. } => {
                start_scoped(
                    &mut self.turns,
                    turn_id,
                    &envelope.thread_id,
                    ConformanceViolation::InvalidTurnTransition(turn_id.clone()),
                )?;
            }
            RuntimeEvent::TurnTerminal { turn_id, .. } => {
                terminal_scoped(
                    &mut self.turns,
                    turn_id,
                    &envelope.thread_id,
                    ConformanceViolation::InvalidTurnTransition(turn_id.clone()),
                    ConformanceViolation::InvalidTurnTransition(turn_id.clone()),
                    ConformanceViolation::TurnThreadMismatch(turn_id.clone()),
                )?;
            }
            RuntimeEvent::ItemStarted {
                turn_id, item_id, ..
            } => {
                start_turn_scoped(
                    &mut self.items,
                    item_id,
                    &envelope.thread_id,
                    turn_id,
                    ConformanceViolation::InvalidItemTransition(item_id.clone()),
                )?;
            }
            RuntimeEvent::ConversationDelta {
                turn_id, item_id, ..
            } => {
                let Some(state) = self.items.get(item_id) else {
                    return Err(ConformanceViolation::DeltaAfterItemTerminal(
                        item_id.clone(),
                    ));
                };
                if state.thread_id != envelope.thread_id || state.turn_id != *turn_id {
                    return Err(ConformanceViolation::ItemParentMismatch(item_id.clone()));
                }
                if state.phase != Phase::Active {
                    return Err(ConformanceViolation::DeltaAfterItemTerminal(
                        item_id.clone(),
                    ));
                }
            }
            RuntimeEvent::ItemTerminal {
                turn_id, item_id, ..
            } => {
                terminal_turn_scoped(
                    &mut self.items,
                    item_id,
                    &envelope.thread_id,
                    turn_id,
                    ConformanceViolation::InvalidItemTransition(item_id.clone()),
                    ConformanceViolation::ItemParentMismatch(item_id.clone()),
                )?;
            }
            RuntimeEvent::InteractionRequested {
                turn_id,
                interaction_id,
                ..
            } => {
                start_turn_scoped(
                    &mut self.interactions,
                    interaction_id,
                    &envelope.thread_id,
                    turn_id,
                    ConformanceViolation::InvalidInteractionTransition(interaction_id.clone()),
                )?;
            }
            RuntimeEvent::InteractionTerminal {
                turn_id,
                interaction_id,
                ..
            } => {
                terminal_turn_scoped(
                    &mut self.interactions,
                    interaction_id,
                    &envelope.thread_id,
                    turn_id,
                    ConformanceViolation::InvalidInteractionTransition(interaction_id.clone()),
                    ConformanceViolation::InteractionParentMismatch(interaction_id.clone()),
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn finish(self) -> Result<(), ConformanceViolation> {
        let all_terminal = self
            .operations
            .values()
            .chain(self.turns.values())
            .all(|state| state.phase == Phase::Terminal)
            && self
                .items
                .values()
                .chain(self.interactions.values())
                .all(|state| state.phase == Phase::Terminal);
        if all_terminal {
            Ok(())
        } else {
            Err(ConformanceViolation::MissingTerminal)
        }
    }
}

fn start_scoped<K: Ord + Clone>(
    states: &mut BTreeMap<K, ScopedPhase>,
    key: &K,
    thread_id: &RuntimeThreadId,
    error: ConformanceViolation,
) -> Result<(), ConformanceViolation> {
    if states
        .insert(
            key.clone(),
            ScopedPhase {
                phase: Phase::Active,
                thread_id: thread_id.clone(),
            },
        )
        .is_some()
    {
        return Err(error);
    }
    Ok(())
}

fn terminal_scoped<K: Ord + Clone>(
    states: &mut BTreeMap<K, ScopedPhase>,
    key: &K,
    thread_id: &RuntimeThreadId,
    missing: ConformanceViolation,
    duplicate: ConformanceViolation,
    mismatch: ConformanceViolation,
) -> Result<(), ConformanceViolation> {
    match states.get_mut(key) {
        Some(state) if state.thread_id != *thread_id => Err(mismatch),
        Some(ScopedPhase {
            phase: phase @ Phase::Active,
            ..
        }) => {
            *phase = Phase::Terminal;
            Ok(())
        }
        Some(ScopedPhase {
            phase: Phase::Terminal,
            ..
        }) => Err(duplicate),
        None => Err(missing),
    }
}

fn start_turn_scoped<K: Ord + Clone>(
    states: &mut BTreeMap<K, TurnScopedPhase>,
    key: &K,
    thread_id: &RuntimeThreadId,
    turn_id: &RuntimeTurnId,
    error: ConformanceViolation,
) -> Result<(), ConformanceViolation> {
    if states
        .insert(
            key.clone(),
            TurnScopedPhase {
                phase: Phase::Active,
                thread_id: thread_id.clone(),
                turn_id: turn_id.clone(),
            },
        )
        .is_some()
    {
        return Err(error);
    }
    Ok(())
}

fn terminal_turn_scoped<K: Ord + Clone>(
    states: &mut BTreeMap<K, TurnScopedPhase>,
    key: &K,
    thread_id: &RuntimeThreadId,
    turn_id: &RuntimeTurnId,
    transition_error: ConformanceViolation,
    mismatch: ConformanceViolation,
) -> Result<(), ConformanceViolation> {
    match states.get_mut(key) {
        Some(state) if state.thread_id != *thread_id || state.turn_id != *turn_id => Err(mismatch),
        Some(TurnScopedPhase {
            phase: phase @ Phase::Active,
            ..
        }) => {
            *phase = Phase::Terminal;
            Ok(())
        }
        Some(TurnScopedPhase {
            phase: Phase::Terminal,
            ..
        })
        | None => Err(transition_error),
    }
}

#[derive(Default)]
pub struct RecordingEventSink {
    pub events: tokio::sync::Mutex<Vec<agentdash_agent_runtime_contract::DriverEventEnvelope>>,
}

#[async_trait]
impl DriverEventSink for RecordingEventSink {
    async fn emit(
        &self,
        event: agentdash_agent_runtime_contract::DriverEventEnvelope,
    ) -> Result<(), DriverError> {
        self.events.lock().await.push(event);
        Ok(())
    }
}

#[async_trait]
pub trait SideEffectProbe: Send + Sync {
    async fn side_effect_count(&self) -> usize;
}

/// Verifies that an unsupported dispatch is rejected and produces no observable side effect.
pub async fn assert_unsupported_before_side_effect<D>(
    driver: &D,
    command: DriverCommandEnvelope,
) -> Result<(), HarnessError>
where
    D: AgentRuntimeDriver + SideEffectProbe,
{
    let before = driver.side_effect_count().await;
    let sink = Arc::new(RecordingEventSink::default());
    let result = driver.dispatch(command, sink).await;
    let after = driver.side_effect_count().await;
    if !matches!(result, Err(DriverError::Unsupported { .. })) {
        return Err(HarnessError::DidNotReturnUnsupported);
    }
    if before != after {
        return Err(HarnessError::UnsupportedProducedSideEffect);
    }
    Ok(())
}

/// A minimal unsupported driver useful for adopting the shared test suite.
pub struct UnsupportedRecordingDriver {
    pub descriptor: agentdash_agent_runtime_contract::RuntimeDescriptor,
    side_effects: AtomicUsize,
}

impl UnsupportedRecordingDriver {
    pub fn new(descriptor: agentdash_agent_runtime_contract::RuntimeDescriptor) -> Self {
        Self {
            descriptor,
            side_effects: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl SideEffectProbe for UnsupportedRecordingDriver {
    async fn side_effect_count(&self) -> usize {
        self.side_effects.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AgentRuntimeDriver for UnsupportedRecordingDriver {
    async fn describe(
        &self,
        _request: agentdash_agent_runtime_contract::DriverDescribeRequest,
    ) -> Result<agentdash_agent_runtime_contract::RuntimeDescriptor, DriverError> {
        Ok(self.descriptor.clone())
    }

    async fn bind(
        &self,
        _request: agentdash_agent_runtime_contract::DriverBindRequest,
    ) -> Result<agentdash_agent_runtime_contract::DriverBinding, DriverError> {
        Err(DriverError::Unsupported {
            reason: "binding is unsupported".to_string(),
        })
    }

    async fn dispatch(
        &self,
        _command: DriverCommandEnvelope,
        _sink: Arc<dyn DriverEventSink>,
    ) -> Result<DriverDispatchReceipt, DriverError> {
        Err(DriverError::Unsupported {
            reason: "command is unsupported".to_string(),
        })
    }

    async fn inspect(
        &self,
        _query: agentdash_agent_runtime_contract::DriverInspectionQuery,
    ) -> Result<agentdash_agent_runtime_contract::DriverInspection, DriverError> {
        Err(DriverError::Unsupported {
            reason: "inspection is unsupported".to_string(),
        })
    }
}

pub fn set<T: Ord>(values: impl IntoIterator<Item = T>) -> BTreeSet<T> {
    values.into_iter().collect()
}

#[cfg(test)]
mod tests;
