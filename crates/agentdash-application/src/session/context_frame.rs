use agentdash_spi::hooks::{ContextFrame, ContextFrameSection, RuntimeEventSource};

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
