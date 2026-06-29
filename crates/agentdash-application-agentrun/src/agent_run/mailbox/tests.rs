use super::*;

#[test]
fn mailbox_command_target_can_be_address_first_without_message_stream() {
    let address = AgentRunRuntimeAddress {
        run_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        frame_id: Uuid::new_v4(),
    };

    let target = AgentRunMailboxCommandTarget::new(address.clone());

    assert_eq!(target.address, address);
    assert!(target.message_stream.is_none());
}

#[test]
fn runtime_session_adapter_keeps_session_as_message_stream_ref() {
    let run_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let frame_id = Uuid::new_v4();

    let target = AgentRunMailboxCommandTarget::from_runtime_session_adapter(
        run_id,
        agent_id,
        frame_id,
        "runtime-session-1",
    );

    assert_eq!(
        target.address,
        AgentRunRuntimeAddress {
            run_id,
            agent_id,
            frame_id,
        }
    );
    assert_eq!(
        target.message_stream,
        Some(MessageStreamProjectionRef {
            runtime_session_id: "runtime-session-1".to_string(),
            trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
        })
    );
}

#[test]
fn mailbox_source_identity_dedup_prefers_source_ref_and_correlation_ref() {
    let source = MailboxSourceIdentity::new("routine", "trigger", "routine")
        .with_source_ref("routine-execution-1")
        .with_correlation_ref("trigger-1");

    assert_eq!(
        mailbox_source_identity_dedup_key(&source).as_deref(),
        Some("source:routine:trigger:ref:routine-execution-1:correlation:trigger-1")
    );
}

#[test]
fn mailbox_source_identity_dedup_can_use_correlation_without_source_ref() {
    let source = MailboxSourceIdentity::new("companion", "parent_response", "agent")
        .with_correlation_ref("gate-1");

    assert_eq!(
        mailbox_source_identity_dedup_key(&source).as_deref(),
        Some("source:companion:parent_response:correlation:gate-1")
    );
}

#[test]
fn mailbox_intake_command_prefers_source_identity_dedup() {
    let command = AgentRunMailboxIntakeTargetCommand {
        target: AgentRunMailboxCommandTarget::new(AgentRunRuntimeAddress {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            frame_id: Uuid::new_v4(),
        }),
        origin: MailboxMessageOrigin::Companion,
        source: MailboxSourceIdentity::new("companion", "result", "agent")
            .with_source_ref("gate-1"),
        retain_payload: true,
        schedule_on_submit: false,
        input: Vec::new(),
        client_command_id: "cmd-1".to_string(),
        source_dedup_key: Some("custom-dedup".to_string()),
        executor_config: None,
        identity: None,
        delivery_intent: None,
    };

    assert_eq!(
        command.stable_source_dedup_key().as_deref(),
        Some("source:companion:result:ref:gate-1")
    );
}

#[test]
fn mailbox_intake_command_uses_explicit_source_dedup_without_source_refs() {
    let command = AgentRunMailboxIntakeTargetCommand {
        target: AgentRunMailboxCommandTarget::new(AgentRunRuntimeAddress {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            frame_id: Uuid::new_v4(),
        }),
        origin: MailboxMessageOrigin::Companion,
        source: MailboxSourceIdentity::new("companion", "result", "agent"),
        retain_payload: true,
        schedule_on_submit: false,
        input: Vec::new(),
        client_command_id: "cmd-1".to_string(),
        source_dedup_key: Some("custom-dedup".to_string()),
        executor_config: None,
        identity: None,
        delivery_intent: None,
    };

    assert_eq!(
        command.stable_source_dedup_key().as_deref(),
        Some("custom-dedup")
    );
}
