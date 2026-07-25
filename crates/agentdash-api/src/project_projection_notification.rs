use agentdash_contracts::project::ProjectEventStreamEnvelope;

pub(crate) fn project_id_from_projection_event(event: &ProjectEventStreamEnvelope) -> Option<&str> {
    match event {
        ProjectEventStreamEnvelope::ControlPlaneProjectionChanged(data) => {
            Some(data.project_id.as_str())
        }
        _ => None,
    }
}
