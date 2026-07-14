use agentdash_agent_protocol::{
    ContextDeliveryChannel, ContextDeliveryStatus, ContextFrameKind, ContextFrameSection,
    ContextFrameSource, ContextMessageRole,
};

/// Protocol-neutral facts for one context presentation frame.
///
/// Application source adapters provide these facts; adapters and API transports never construct
/// the resulting `ContextFrame`.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextFrameFacts {
    pub kind: ContextFrameKind,
    pub source: ContextFrameSource,
    pub phase_node: Option<String>,
    pub apply_mode: Option<String>,
    pub delivery_status: ContextDeliveryStatus,
    pub delivery_channel: ContextDeliveryChannel,
    pub message_role: ContextMessageRole,
    pub rendered_text: String,
    pub sections: Vec<ContextFrameSection>,
}

/// Stable inputs supplied by the canonical Runtime operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextProjectionIdentity {
    pub operation_id: String,
    pub source_frame_id: String,
    pub source_frame_revision: u64,
    pub recorded_at_ms: i64,
}
