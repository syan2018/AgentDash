pub mod advance_node;

pub use advance_node::AdvanceLifecycleNodeTool;

use agentdash_spi::SessionHookSnapshot;
use uuid::Uuid;

pub struct ActiveWorkflowLocator {
    pub run_id: Uuid,
    pub step_key: String,
}

pub fn active_workflow_locator_from_snapshot(
    snapshot: &SessionHookSnapshot,
) -> Option<ActiveWorkflowLocator> {
    let aw = snapshot.metadata.as_ref()?.active_workflow.as_ref()?;
    Some(ActiveWorkflowLocator {
        run_id: aw.run_id?,
        step_key: aw.step_key.clone()?,
    })
}
