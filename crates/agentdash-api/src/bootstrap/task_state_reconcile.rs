use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use agentdash_application::session::SessionExecutionState as HubSessionExecutionState;
use agentdash_application::session::SessionHub;
use agentdash_application::task_restart_tracker::RestartTracker;
use agentdash_application::task_state_reconciler::{
    TaskSessionState, TaskSessionStateReader, reconcile_running_tasks_on_boot,
};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::story::{StateChangeRepository, StoryRepository};

pub struct HubSessionStateReader {
    pub hub: SessionHub,
}

#[async_trait]
impl TaskSessionStateReader for HubSessionStateReader {
    async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> Result<TaskSessionState, String> {
        let raw = self
            .hub
            .inspect_session_execution_state(session_id)
            .await
            .map_err(|e| e.to_string())?;

        Ok(match raw {
            HubSessionExecutionState::Idle => TaskSessionState::Idle,
            HubSessionExecutionState::Running { turn_id } => TaskSessionState::Running { turn_id },
            HubSessionExecutionState::Completed { turn_id } => {
                TaskSessionState::Completed { turn_id }
            }
            HubSessionExecutionState::Failed { turn_id, message } => {
                TaskSessionState::Failed { turn_id, message }
            }
            HubSessionExecutionState::Interrupted { turn_id, message } => {
                TaskSessionState::Interrupted { turn_id, message }
            }
        })
    }
}

pub async fn reconcile_task_states_on_boot(
    project_repo: &Arc<dyn ProjectRepository>,
    state_change_repo: &Arc<dyn StateChangeRepository>,
    story_repo: &Arc<dyn StoryRepository>,
    session_binding_repo: &Arc<dyn SessionBindingRepository>,
    session_hub: &SessionHub,
    restart_tracker: &RestartTracker,
) -> Result<()> {
    let session_state_reader = HubSessionStateReader {
        hub: session_hub.clone(),
    };

    reconcile_running_tasks_on_boot(
        project_repo,
        state_change_repo,
        story_repo,
        session_binding_repo,
        &session_state_reader,
        Some(restart_tracker),
        None,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}
