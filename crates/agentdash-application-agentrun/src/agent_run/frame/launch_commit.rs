//! Accepted launch commit adapter for AgentRun/Lifecycle control-plane writes.
//!
//! RuntimeSession launch owns delivery events and stream attachment. This
//! adapter owns accepted AgentFrame revision persistence, LifecycleAgent current
//! delivery binding, and owner-bootstrap status transitions.

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;

use agentdash_application_ports::frame_launch_envelope::{
    AcceptedLaunchCommitInput, AcceptedLaunchCommitOutcome, AcceptedLaunchCommitPort,
    AcceptedLaunchHookRuntimeSync,
};
use agentdash_domain::DomainError;
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, DeliveryBindingStatus, LifecycleAgent,
    LifecycleAgentRepository, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::CapabilityState;
use async_trait::async_trait;
use uuid::Uuid;

use crate::agent_run::AgentFrameRuntimeTarget;
use crate::agent_run::frame::builder::AgentFrameBuilder;
use crate::agent_run::runtime_capability::capability_state_to_frame_surfaces;

#[derive(Clone)]
pub struct AgentRunAcceptedLaunchCommitAdapter {
    frame_repo: Option<Arc<dyn AgentFrameRepository>>,
    anchor_repo: Option<Arc<dyn RuntimeSessionExecutionAnchorRepository>>,
    agent_repo: Option<Arc<dyn LifecycleAgentRepository>>,
    hook_runtime_sync: Option<Arc<dyn AcceptedLaunchHookRuntimeSync>>,
}

#[derive(Clone)]
pub struct AgentRunAcceptedLaunchCommitDeps {
    pub frame_repo: Option<Arc<dyn AgentFrameRepository>>,
    pub anchor_repo: Option<Arc<dyn RuntimeSessionExecutionAnchorRepository>>,
    pub agent_repo: Option<Arc<dyn LifecycleAgentRepository>>,
    pub hook_runtime_sync: Option<Arc<dyn AcceptedLaunchHookRuntimeSync>>,
}

impl AgentRunAcceptedLaunchCommitAdapter {
    pub fn new(deps: AgentRunAcceptedLaunchCommitDeps) -> Self {
        Self {
            frame_repo: deps.frame_repo,
            anchor_repo: deps.anchor_repo,
            agent_repo: deps.agent_repo,
            hook_runtime_sync: deps.hook_runtime_sync,
        }
    }

    pub async fn agent_needs_bootstrap(&self, runtime_session_id: &str) -> bool {
        match self.resolve_current_agent_frame(runtime_session_id).await {
            Ok(Some((_anchor, agent, _frame))) => agent.needs_bootstrap(),
            _ => false,
        }
    }

    pub async fn mark_agent_bootstrapped(&self, runtime_session_id: &str) {
        let Some(agent_repo) = self.agent_repo.as_ref() else {
            return;
        };
        let (anchor, mut agent) = match self.resolve_current_agent_frame(runtime_session_id).await {
            Ok(Some((anchor, agent, _frame))) => (anchor, agent),
            _ => return,
        };
        agent.mark_bootstrapped();
        if let Err(error) = agent_repo.update(&agent).await {
            let diagnostic_context =
                DiagnosticErrorContext::new("agent_run.launch_commit", "mark_bootstrapped");
            diag_error!(Warn, Subsystem::AgentRun,
                context = &diagnostic_context,
                error = &error,
                session_id = %runtime_session_id,
                run_id = %anchor.run_id,
                agent_id = %agent.id,
                "Failed to mark AgentRun agent bootstrapped"
            );
        }
    }

    pub async fn commit_accepted_launch(
        &self,
        input: AcceptedLaunchCommitInput,
    ) -> AcceptedLaunchCommitOutcome {
        let (Some(frame_repo), Some(anchor_repo), Some(agent_repo)) = (
            self.frame_repo.as_ref(),
            self.anchor_repo.as_ref(),
            self.agent_repo.as_ref(),
        ) else {
            return AcceptedLaunchCommitOutcome::with_diagnostic(
                "AgentRun accepted launch commit repositories 未完整注入",
            );
        };

        if let Some(pending_frame) = input.pending_frame {
            return self
                .commit_pending_frame(
                    frame_repo.as_ref(),
                    anchor_repo.as_ref(),
                    agent_repo.as_ref(),
                    input.runtime_session_id.as_str(),
                    input.turn_id.as_str(),
                    pending_frame,
                    &input.accepted_capability_state,
                )
                .await;
        }

        self.commit_revision_from_current_frame(
            frame_repo.as_ref(),
            anchor_repo.as_ref(),
            agent_repo.as_ref(),
            input.runtime_session_id.as_str(),
            input.turn_id.as_str(),
            &input.accepted_capability_state,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn commit_pending_frame(
        &self,
        frame_repo: &dyn AgentFrameRepository,
        anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
        agent_repo: &dyn LifecycleAgentRepository,
        runtime_session_id: &str,
        turn_id: &str,
        mut pending_frame: AgentFrame,
        accepted_capability_state: &CapabilityState,
    ) -> AcceptedLaunchCommitOutcome {
        let mut outcome = AcceptedLaunchCommitOutcome::empty();
        let surfaces = capability_state_to_frame_surfaces(accepted_capability_state);
        pending_frame.effective_capability_json = surfaces.effective_capability_json;
        pending_frame.vfs_surface_json = surfaces.vfs_surface_json;
        pending_frame.mcp_surface_json = surfaces.mcp_surface_json;
        match frame_repo.create(&pending_frame).await {
            Ok(()) => {
                diag!(Debug, Subsystem::AgentRun,

                    session_id = %runtime_session_id,
                    agent_id = %pending_frame.agent_id,
                    revision = pending_frame.revision,
                    "accepted pending AgentFrame revision 已写入"
                );
                outcome.frame_id = Some(pending_frame.id);
                outcome.agent_id = Some(pending_frame.agent_id);
                outcome.wrote_frame_revision = true;
            }
            Err(error) => {
                let diagnostic = format!("accepted pending AgentFrame revision 写入失败: {error}");
                let diagnostic_context =
                    DiagnosticErrorContext::new("agent_run.launch_commit", "pending_frame_create");
                diag_error!(Warn, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &error,
                    session_id = %runtime_session_id,
                    turn_id = %turn_id,
                    agent_id = %pending_frame.agent_id,
                    frame_id = %pending_frame.id,
                    frame_revision = pending_frame.revision,
                    "Failed to write accepted pending AgentFrame revision"
                );
                outcome.diagnostics.push(diagnostic);
                return outcome;
            }
        }

        match self
            .bind_current_delivery(
                agent_repo,
                anchor_repo,
                runtime_session_id,
                pending_frame.agent_id,
            )
            .await
        {
            Ok(bound) => outcome.bound_current_delivery = bound,
            Err(error) => outcome.diagnostics.push(error),
        }
        if self
            .sync_hook_runtime_target(runtime_session_id, turn_id, pending_frame.id)
            .await
        {
            outcome.synced_hook_runtime_target = true;
        }
        outcome
    }

    async fn commit_revision_from_current_frame(
        &self,
        frame_repo: &dyn AgentFrameRepository,
        anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
        agent_repo: &dyn LifecycleAgentRepository,
        runtime_session_id: &str,
        turn_id: &str,
        accepted_capability_state: &CapabilityState,
    ) -> AcceptedLaunchCommitOutcome {
        let (anchor, current_frame) = match resolve_current_agent_frame_for_runtime_session(
            runtime_session_id,
            anchor_repo,
            agent_repo,
            frame_repo,
        )
        .await
        {
            Ok(Some((anchor, _agent, current_frame))) => (anchor, current_frame),
            Ok(None) => return AcceptedLaunchCommitOutcome::empty(),
            Err(error) => {
                let diagnostic = format!("查找 session 关联的 AgentFrame 失败: {error}");
                let diagnostic_context =
                    DiagnosticErrorContext::new("agent_run.launch_commit", "resolve_current_frame");
                diag_error!(Warn, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &error,
                    session_id = %runtime_session_id,
                    turn_id = %turn_id,
                    "Failed to resolve AgentFrame for accepted launch commit"
                );
                return AcceptedLaunchCommitOutcome::with_diagnostic(diagnostic);
            }
        };

        let current_frame_id = current_frame.id;
        let current_frame_agent_id = current_frame.agent_id;
        let current_frame_revision = current_frame.revision;
        let mut builder = AgentFrameBuilder::new(current_frame_agent_id)
            .with_capability_state(accepted_capability_state)
            .with_created_by("session_launch", Some(runtime_session_id.to_string()));
        if let Some(ctx) = current_frame.context_slice_json {
            builder = builder.with_context(ctx);
        }
        if let Some(profile) = current_frame.execution_profile_json {
            builder = builder.with_execution_profile_raw(profile);
        }

        let mut outcome = AcceptedLaunchCommitOutcome::empty();
        match builder.build(frame_repo).await {
            Ok(frame) => {
                diag!(Debug, Subsystem::AgentRun,

                    session_id = %runtime_session_id,
                    agent_id = %frame.agent_id,
                    revision = frame.revision,
                    "accepted AgentFrame revision 已写入"
                );
                outcome.frame_id = Some(frame.id);
                outcome.agent_id = Some(frame.agent_id);
                outcome.wrote_frame_revision = true;
                match self
                    .bind_current_delivery_with_anchor(agent_repo, &anchor, frame.agent_id)
                    .await
                {
                    Ok(bound) => outcome.bound_current_delivery = bound,
                    Err(error) => outcome.diagnostics.push(error),
                }
                if self
                    .sync_hook_runtime_target(runtime_session_id, turn_id, frame.id)
                    .await
                {
                    outcome.synced_hook_runtime_target = true;
                }
            }
            Err(error) => {
                let diagnostic = format!("accepted AgentFrame revision 写入失败: {error}");
                let diagnostic_context = DiagnosticErrorContext::new(
                    "agent_run.launch_commit",
                    "current_frame_revision",
                );
                diag_error!(Warn, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &error,
                    session_id = %runtime_session_id,
                    turn_id = %turn_id,
                    run_id = %anchor.run_id,
                    agent_id = %current_frame_agent_id,
                    frame_id = %current_frame_id,
                    frame_revision = current_frame_revision,
                    "Failed to write accepted AgentFrame revision"
                );
                outcome.diagnostics.push(diagnostic);
            }
        }
        outcome
    }

    async fn bind_current_delivery(
        &self,
        agent_repo: &dyn LifecycleAgentRepository,
        anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
        runtime_session_id: &str,
        agent_id: Uuid,
    ) -> Result<bool, String> {
        match anchor_repo.find_by_session(runtime_session_id).await {
            Ok(Some(anchor)) => {
                self.bind_current_delivery_with_anchor(agent_repo, &anchor, agent_id)
                    .await
            }
            Ok(None) => {
                diag!(Warn, Subsystem::AgentRun,

                    session_id = %runtime_session_id,
                    "accepted pending AgentFrame 已写入但缺少 current delivery anchor"
                );
                Ok(false)
            }
            Err(error) => {
                let diagnostic = format!(
                    "accepted pending AgentFrame 查询 current delivery anchor 失败: {error}"
                );
                let diagnostic_context = DiagnosticErrorContext::new(
                    "agent_run.launch_commit",
                    "current_delivery_anchor",
                );
                diag_error!(Warn, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &error,
                    session_id = %runtime_session_id,
                    agent_id = %agent_id,
                    "Failed to query current delivery anchor for accepted pending AgentFrame"
                );
                Err(diagnostic)
            }
        }
    }

    async fn bind_current_delivery_with_anchor(
        &self,
        agent_repo: &dyn LifecycleAgentRepository,
        anchor: &RuntimeSessionExecutionAnchor,
        agent_id: Uuid,
    ) -> Result<bool, String> {
        let mut agent = match agent_repo.get(agent_id).await {
            Ok(Some(agent)) => agent,
            Ok(None) => return Ok(false),
            Err(error) => {
                return Err(format!(
                    "查询 LifecycleAgent current delivery 失败: {error}"
                ));
            }
        };
        agent.bind_current_delivery_from_anchor(
            anchor,
            DeliveryBindingStatus::Running,
            chrono::Utc::now(),
        );
        if let Err(error) = agent_repo.update(&agent).await {
            let diagnostic = format!("同步 accepted current delivery 失败: {error}");
            let diagnostic_context =
                DiagnosticErrorContext::new("agent_run.launch_commit", "bind_current_delivery");
            diag_error!(Warn, Subsystem::AgentRun,
                context = &diagnostic_context,
                error = &error,
                session_id = %anchor.runtime_session_id,
                run_id = %anchor.run_id,
                agent_id = %agent.id,
                launch_frame_id = %anchor.launch_frame_id,
                "Failed to sync accepted current delivery"
            );
            return Err(diagnostic);
        }
        Ok(true)
    }

    async fn sync_hook_runtime_target(
        &self,
        runtime_session_id: &str,
        turn_id: &str,
        frame_id: Uuid,
    ) -> bool {
        let Some(sync) = self.hook_runtime_sync.as_ref() else {
            return false;
        };
        let target = AgentFrameRuntimeTarget {
            frame_id,
            delivery_runtime_session_id: runtime_session_id.to_string(),
        };
        match sync
            .sync_accepted_launch_hook_runtime(target, turn_id)
            .await
        {
            Ok(()) => true,
            Err(error) => {
                let diagnostic_context =
                    DiagnosticErrorContext::new("agent_run.launch_commit", "hook_runtime_sync");
                diag_error!(Warn, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &error,
                    session_id = %runtime_session_id,
                    turn_id = %turn_id,
                    %frame_id,
                    "Failed to sync accepted AgentFrame hook runtime target"
                );
                false
            }
        }
    }

    async fn resolve_current_agent_frame(
        &self,
        runtime_session_id: &str,
    ) -> Result<
        Option<(
            RuntimeSessionExecutionAnchor,
            agentdash_domain::workflow::LifecycleAgent,
            AgentFrame,
        )>,
        agentdash_domain::DomainError,
    > {
        let (Some(frame_repo), Some(anchor_repo), Some(agent_repo)) = (
            self.frame_repo.as_ref(),
            self.anchor_repo.as_ref(),
            self.agent_repo.as_ref(),
        ) else {
            return Ok(None);
        };
        resolve_current_agent_frame_for_runtime_session(
            runtime_session_id,
            anchor_repo.as_ref(),
            agent_repo.as_ref(),
            frame_repo.as_ref(),
        )
        .await
    }
}

#[async_trait]
impl AcceptedLaunchCommitPort for AgentRunAcceptedLaunchCommitAdapter {
    async fn agent_needs_bootstrap(&self, runtime_session_id: &str) -> bool {
        AgentRunAcceptedLaunchCommitAdapter::agent_needs_bootstrap(self, runtime_session_id).await
    }

    async fn mark_agent_bootstrapped(&self, runtime_session_id: &str) {
        AgentRunAcceptedLaunchCommitAdapter::mark_agent_bootstrapped(self, runtime_session_id)
            .await;
    }

    async fn commit_accepted_launch(
        &self,
        input: AcceptedLaunchCommitInput,
    ) -> AcceptedLaunchCommitOutcome {
        AgentRunAcceptedLaunchCommitAdapter::commit_accepted_launch(self, input).await
    }
}

pub fn accepted_launch_commit_port(
    frame_repo: Option<Arc<dyn AgentFrameRepository>>,
    anchor_repo: Option<Arc<dyn RuntimeSessionExecutionAnchorRepository>>,
    agent_repo: Option<Arc<dyn LifecycleAgentRepository>>,
    hook_runtime_sync: Option<Arc<dyn AcceptedLaunchHookRuntimeSync>>,
) -> Arc<dyn AcceptedLaunchCommitPort> {
    Arc::new(AgentRunAcceptedLaunchCommitAdapter::new(
        AgentRunAcceptedLaunchCommitDeps {
            frame_repo,
            anchor_repo,
            agent_repo,
            hook_runtime_sync,
        },
    ))
}

async fn resolve_current_agent_frame_for_runtime_session(
    runtime_session_id: &str,
    anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
    agent_repo: &dyn LifecycleAgentRepository,
    frame_repo: &dyn AgentFrameRepository,
) -> Result<Option<(RuntimeSessionExecutionAnchor, LifecycleAgent, AgentFrame)>, DomainError> {
    let Some(anchor) = anchor_repo.find_by_session(runtime_session_id).await? else {
        return Ok(None);
    };
    let Some(agent) = agent_repo.get(anchor.agent_id).await? else {
        return Ok(None);
    };
    let Some(frame) = frame_repo.get_current(agent.id).await? else {
        return Ok(None);
    };
    Ok(Some((anchor, agent, frame)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        AgentFrame, AgentSource, LifecycleAgent, RuntimeSessionExecutionAnchor,
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemoryFrameRepo {
        frames: Mutex<Vec<AgentFrame>>,
    }

    #[async_trait]
    impl AgentFrameRepository for MemoryFrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.frames.lock().unwrap().push(frame.clone());
            Ok(())
        }

        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .lock()
                .unwrap()
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned())
        }

        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            let frames = self.frames.lock().unwrap();
            Ok(frames
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .max_by_key(|frame| frame.revision)
                .cloned())
        }

        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn append_visible_canvas_mount(
            &self,
            _frame_id: Uuid,
            _mount_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryAgentRepo {
        agents: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait]
    impl LifecycleAgentRepository for MemoryAgentRepo {
        async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            self.agents.lock().unwrap().push(agent.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .unwrap()
                .iter()
                .find(|agent| agent.id == id)
                .cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .unwrap()
                .iter()
                .filter(|agent| agent.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            let mut agents = self.agents.lock().unwrap();
            if let Some(existing) = agents.iter_mut().find(|existing| existing.id == agent.id) {
                *existing = agent.clone();
            } else {
                agents.push(agent.clone());
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryAnchorRepo {
        anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for MemoryAnchorRepo {
        async fn create_once(
            &self,
            anchor: &RuntimeSessionExecutionAnchor,
        ) -> Result<(), DomainError> {
            let mut anchors = self.anchors.lock().unwrap();
            if let Some(existing) = anchors
                .iter()
                .find(|item| item.runtime_session_id == anchor.runtime_session_id)
            {
                if existing.has_same_launch_coordinates_as(anchor) {
                    return Ok(());
                }
                return Err(existing.immutable_conflict(anchor));
            }
            anchors.push(anchor.clone());
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.anchors
                .lock()
                .unwrap()
                .retain(|anchor| anchor.runtime_session_id != runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .find(|anchor| anchor.runtime_session_id == runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| runtime_session_ids.contains(&anchor.runtime_session_id))
                .cloned()
                .collect())
        }
    }

    #[tokio::test]
    async fn accepted_launch_commit_writes_frame_and_binds_current_delivery() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let mut agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent);
        let launch_frame = AgentFrame::new_initial(agent.id);
        let pending_frame = AgentFrame::new_revision(agent.id, 2, "test");
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-a",
            run_id,
            launch_frame.id,
            agent.id,
        );

        let frame_repo = Arc::new(MemoryFrameRepo::default());
        frame_repo.create(&launch_frame).await.unwrap();
        let agent_repo = Arc::new(MemoryAgentRepo::default());
        agent_repo.create(&agent).await.unwrap();
        let anchor_repo = Arc::new(MemoryAnchorRepo::default());
        anchor_repo.create_once(&anchor).await.unwrap();

        let adapter = AgentRunAcceptedLaunchCommitAdapter::new(AgentRunAcceptedLaunchCommitDeps {
            frame_repo: Some(frame_repo.clone()),
            anchor_repo: Some(anchor_repo),
            agent_repo: Some(agent_repo.clone()),
            hook_runtime_sync: None,
        });

        let outcome = adapter
            .commit_accepted_launch(AcceptedLaunchCommitInput {
                runtime_session_id: "runtime-a".to_string(),
                turn_id: "turn-a".to_string(),
                pending_frame: Some(pending_frame.clone()),
                accepted_capability_state: CapabilityState::default(),
            })
            .await;

        assert!(outcome.wrote_frame_revision);
        assert!(outcome.bound_current_delivery);
        assert_eq!(outcome.frame_id, Some(pending_frame.id));
        agent = agent_repo.get(agent.id).await.unwrap().unwrap();
        let binding = agent.current_delivery.expect("current delivery");
        assert_eq!(binding.runtime_session_id, "runtime-a");
        assert_eq!(binding.status, DeliveryBindingStatus::Running);
    }

    #[tokio::test]
    async fn bootstrap_status_is_owned_by_launch_commit_adapter() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent);
        let frame = AgentFrame::new_initial(agent.id);
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-bootstrap",
            run_id,
            frame.id,
            agent.id,
        );

        let frame_repo = Arc::new(MemoryFrameRepo::default());
        frame_repo.create(&frame).await.unwrap();
        let agent_repo = Arc::new(MemoryAgentRepo::default());
        agent_repo.create(&agent).await.unwrap();
        let anchor_repo = Arc::new(MemoryAnchorRepo::default());
        anchor_repo.create_once(&anchor).await.unwrap();
        let adapter = AgentRunAcceptedLaunchCommitAdapter::new(AgentRunAcceptedLaunchCommitDeps {
            frame_repo: Some(frame_repo),
            anchor_repo: Some(anchor_repo),
            agent_repo: Some(agent_repo.clone()),
            hook_runtime_sync: None,
        });

        assert!(adapter.agent_needs_bootstrap("runtime-bootstrap").await);
        adapter.mark_agent_bootstrapped("runtime-bootstrap").await;

        let updated = agent_repo.get(agent.id).await.unwrap().unwrap();
        assert!(!updated.needs_bootstrap());
    }
}
