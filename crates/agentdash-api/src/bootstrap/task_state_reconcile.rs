use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use agentdash_application::task_restart_tracker::RestartTracker;
use agentdash_application::task_state_reconciler::{
    SessionExecutionState, SessionExecutionStateReader, reconcile_running_tasks_on_boot,
};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_application::session::ExecutorHub;
use agentdash_application::session::SessionExecutionState as HubSessionExecutionState;

struct HubSessionStateReader<'a> {
    hub: &'a ExecutorHub,
}

#[async_trait]
impl SessionExecutionStateReader for HubSessionStateReader<'_> {
    async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> Result<SessionExecutionState, String> {
        let raw = self
            .hub
            .inspect_session_execution_state(session_id)
            .await
            .map_err(|e| e.to_string())?;

        Ok(match raw {
            HubSessionExecutionState::Idle => SessionExecutionState::Idle,
            HubSessionExecutionState::Running { turn_id } => {
                SessionExecutionState::Running { turn_id }
            }
            HubSessionExecutionState::Completed { turn_id } => {
                SessionExecutionState::Completed { turn_id }
            }
            HubSessionExecutionState::Failed { turn_id, message } => {
                SessionExecutionState::Failed { turn_id, message }
            }
            HubSessionExecutionState::Interrupted { turn_id, message } => {
                SessionExecutionState::Interrupted { turn_id, message }
            }
        })
    }
}

pub async fn reconcile_task_states_on_boot(
    project_repo: &Arc<dyn ProjectRepository>,
    story_repo: &Arc<dyn StoryRepository>,
    task_repo: &Arc<dyn TaskRepository>,
    executor_hub: &ExecutorHub,
    restart_tracker: &RestartTracker,
) -> Result<()> {
    let session_state_reader = HubSessionStateReader { hub: executor_hub };

    reconcile_running_tasks_on_boot(
        project_repo,
        story_repo,
        task_repo,
        &session_state_reader,
        Some(restart_tracker),
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}
