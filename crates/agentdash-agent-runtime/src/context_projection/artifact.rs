use agentdash_agent_protocol::ContextFrame;

/// Immutable presentation half of a compiled Agent Surface artifact.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSurfacePresentationPlan {
    pub digest: String,
    pub source_frame_id: String,
    pub source_frame_revision: u64,
    pub bootstrap_frames: Vec<ContextFrame>,
    pub adoption_frames: Vec<ContextFrame>,
}
