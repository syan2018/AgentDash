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
use agentdash_domain::story::StateChangeRepository;
use agentdash_domain::task::TaskRepository;

struct HubSessionStateReader<'a> {
    hub: &'a SessionHub,
}

#[async_trait]
impl TaskSessionStateReader for HubSessionStateReader<'_> {
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
    task_repo: &Arc<dyn TaskRepository>,
    session_hub: &SessionHub,
    restart_tracker: &RestartTracker,
) -> Result<()> {
    let session_state_reader = HubSessionStateReader { hub: session_hub };

    reconcile_running_tasks_on_boot(
        project_repo,
        state_change_repo,
        task_repo,
        &session_state_reader,
        Some(restart_tracker),
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}
