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
