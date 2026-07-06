use async_trait::async_trait;

/// Port for session runtime to update workspace (AgentRun) title.
///
/// The session runtime knows `session_id` but doesn't own the workspace entity.
/// This port bridges the gap: the AgentRun layer resolves session_id to the
/// agent identity and persists the title on the LifecycleAgent.
#[async_trait]
pub trait WorkspaceTitlePort: Send + Sync {
    /// Update workspace title for the agent run that owns this runtime session.
    ///
    /// Returns `true` if the title was actually updated (priority allows it).
    /// Returns `false` if a higher-priority title source already set the title.
    /// Returns `Err` if the session cannot be resolved to an agent run.
    async fn update_workspace_title(
        &self,
        runtime_session_id: &str,
        title: String,
        title_source: &str,
    ) -> Result<bool, WorkspaceTitleError>;
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceTitleError {
    #[error("cannot resolve runtime session to agent run: {0}")]
    SessionNotResolved(String),
    #[error("workspace title update failed: {0}")]
    Internal(String),
}
