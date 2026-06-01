use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookTurnStartNotice, RuntimeEventSource,
    SharedHookRuntime,
};

pub(crate) trait ContextFramePayload {
    fn id(&self, created_at_ms: i64) -> String;
    fn kind(&self) -> &'static str;
    fn source(&self) -> RuntimeEventSource;
    fn delivery_status(&self) -> String;
    fn sections(&self) -> Vec<ContextFrameSection>;
    fn rendered_text(&self) -> String;

    fn phase_node(&self) -> Option<String> {
        None
    }

    fn apply_mode(&self) -> Option<String> {
        None
    }

    fn delivery_channel(&self) -> &'static str {
        "turn_start"
    }

    fn message_role(&self) -> &'static str {
        "user"
    }
}

pub(crate) fn build_context_frame(payload: &impl ContextFramePayload) -> ContextFrame {
    let created_at_ms = chrono::Utc::now().timestamp_millis();
    ContextFrame {
        id: payload.id(created_at_ms),
        kind: payload.kind().to_string(),
        source: payload.source(),
        phase_node: payload.phase_node(),
        apply_mode: payload.apply_mode(),
        delivery_status: payload.delivery_status(),
        delivery_channel: payload.delivery_channel().to_string(),
        message_role: payload.message_role().to_string(),
        rendered_text: payload.rendered_text(),
        sections: payload.sections(),
        created_at_ms,
    }
}

pub(crate) fn enqueue_context_frame(
    hook_runtime: &SharedHookRuntime,
    frame: &ContextFrame,
) -> bool {
    if frame.rendered_text.trim().is_empty() {
        return false;
    }
    hook_runtime.enqueue_turn_start_notice(HookTurnStartNotice {
        id: frame.id.clone(),
        created_at_ms: frame.created_at_ms,
        source: frame.source.clone(),
        content: frame.rendered_text.clone(),
        context_frame: Some(frame.clone()),
    });
    true
}
