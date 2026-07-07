//! Accepted launch commit adapter for AgentRun/Lifecycle control-plane writes.
//!
//! RuntimeSession launch owns delivery events and stream attachment. This
//! adapter owns accepted AgentFrame revision persistence, AgentRun current
//! delivery binding, and owner-bootstrap status transitions.

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;

use agentdash_agent_protocol::ControlPlaneProjectionChangeReason;
use agentdash_application_ports::accepted_turn_lifecycle::{
    AcceptedTurnLifecycleAdvanceInput, AcceptedTurnLifecycleAdvancePort,
};
use agentdash_application_ports::frame_launch_envelope::{
    AcceptedLaunchCommitInput, AcceptedLaunchCommitOutcome, AcceptedLaunchCommitPort,
    AcceptedLaunchHookRuntimeSync,
};
use agentdash_application_ports::project_projection_notification::{
    ProjectProjectionInvalidation, ProjectProjectionNotificationPort,
};
use agentdash_domain::DomainError;
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentRunDeliveryBinding, AgentRunDeliveryBindingRepository,
    DeliveryBindingStatus, LifecycleAgent, LifecycleAgentRepository, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::{CapabilityState, ConnectorError};
use async_trait::async_trait;
use uuid::Uuid;

use crate::agent_run::AgentFrameRuntimeTarget;
use crate::agent_run::frame::builder::AgentFrameBuilder;
use crate::agent_run::runtime_capability::capability_state_to_frame_surfaces;

#[derive(Clone)]
pub struct AgentRunAcceptedLaunchCommitAdapter {
    frame_repo: Option<Arc<dyn AgentFrameRepository>>,
    anchor_repo: Option<Arc<dyn RuntimeSessionExecutionAnchorRepository>>,
    delivery_binding_repo: Option<Arc<dyn AgentRunDeliveryBindingRepository>>,
    agent_repo: Option<Arc<dyn LifecycleAgentRepository>>,
    lifecycle_advance: Option<Arc<dyn AcceptedTurnLifecycleAdvancePort>>,
    hook_runtime_sync: Option<Arc<dyn AcceptedLaunchHookRuntimeSync>>,
    project_projection_notifications: Option<Arc<dyn ProjectProjectionNotificationPort>>,
}

#[derive(Clone)]
pub struct AgentRunAcceptedLaunchCommitDeps {
    pub frame_repo: Option<Arc<dyn AgentFrameRepository>>,
    pub anchor_repo: Option<Arc<dyn RuntimeSessionExecutionAnchorRepository>>,
    pub delivery_binding_repo: Option<Arc<dyn AgentRunDeliveryBindingRepository>>,
    pub agent_repo: Option<Arc<dyn LifecycleAgentRepository>>,
    pub lifecycle_advance: Option<Arc<dyn AcceptedTurnLifecycleAdvancePort>>,
    pub hook_runtime_sync: Option<Arc<dyn AcceptedLaunchHookRuntimeSync>>,
    pub project_projection_notifications: Option<Arc<dyn ProjectProjectionNotificationPort>>,
}

struct CurrentFrameLaunchCommitContext<'a> {
    frame_repo: &'a dyn AgentFrameRepository,
    anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    agent_repo: &'a dyn LifecycleAgentRepository,
    delivery_binding_repo: &'a dyn AgentRunDeliveryBindingRepository,
    lifecycle_advance: &'a dyn AcceptedTurnLifecycleAdvancePort,
    runtime_session_id: &'a str,
    turn_id: &'a str,
    accepted_capability_state: &'a CapabilityState,
}

impl AgentRunAcceptedLaunchCommitAdapter {
    pub fn new(deps: AgentRunAcceptedLaunchCommitDeps) -> Self {
        Self {
            frame_repo: deps.frame_repo,
            anchor_repo: deps.anchor_repo,
            delivery_binding_repo: deps.delivery_binding_repo,
            agent_repo: deps.agent_repo,
            lifecycle_advance: deps.lifecycle_advance,
            hook_runtime_sync: deps.hook_runtime_sync,
            project_projection_notifications: deps.project_projection_notifications,
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
    ) -> Result<AcceptedLaunchCommitOutcome, ConnectorError> {
        let (
            Some(frame_repo),
            Some(anchor_repo),
            Some(delivery_binding_repo),
            Some(agent_repo),
            Some(lifecycle_advance),
        ) = (
            self.frame_repo.as_ref(),
            self.anchor_repo.as_ref(),
            self.delivery_binding_repo.as_ref(),
            self.agent_repo.as_ref(),
            self.lifecycle_advance.as_ref(),
        )
        else {
            return Err(connector_error(
                "AgentRun accepted launch commit 依赖未完整注入",
            ));
        };

        if let Some(pending_frame) = input.pending_frame {
            return self
                .commit_pending_frame(
                    frame_repo.as_ref(),
                    anchor_repo.as_ref(),
                    agent_repo.as_ref(),
                    delivery_binding_repo.as_ref(),
                    lifecycle_advance.as_ref(),
                    input.runtime_session_id.as_str(),
                    input.turn_id.as_str(),
                    pending_frame,
                    &input.accepted_capability_state,
                )
                .await;
        }

        self.commit_revision_from_current_frame(CurrentFrameLaunchCommitContext {
            frame_repo: frame_repo.as_ref(),
            anchor_repo: anchor_repo.as_ref(),
            agent_repo: agent_repo.as_ref(),
            delivery_binding_repo: delivery_binding_repo.as_ref(),
            lifecycle_advance: lifecycle_advance.as_ref(),
            runtime_session_id: input.runtime_session_id.as_str(),
            turn_id: input.turn_id.as_str(),
            accepted_capability_state: &input.accepted_capability_state,
        })
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn commit_pending_frame(
        &self,
        frame_repo: &dyn AgentFrameRepository,
        anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
        agent_repo: &dyn LifecycleAgentRepository,
        delivery_binding_repo: &dyn AgentRunDeliveryBindingRepository,
        lifecycle_advance: &dyn AcceptedTurnLifecycleAdvancePort,
        runtime_session_id: &str,
        turn_id: &str,
        mut pending_frame: AgentFrame,
        accepted_capability_state: &CapabilityState,
    ) -> Result<AcceptedLaunchCommitOutcome, ConnectorError> {
        let mut outcome = AcceptedLaunchCommitOutcome::empty();
        let anchor = self
            .load_anchor_for_session(anchor_repo, runtime_session_id, turn_id)
            .await?;
        if anchor.agent_id != pending_frame.agent_id {
            return Err(connector_error(format!(
                "accepted pending AgentFrame agent_id 与 RuntimeSession anchor 不一致: session_id={runtime_session_id}, turn_id={turn_id}, anchor_agent_id={}, pending_agent_id={}",
                anchor.agent_id, pending_frame.agent_id
            )));
        }
        apply_accepted_capability_surface(&mut pending_frame, accepted_capability_state);
        if let Err(error) = frame_repo.create(&pending_frame).await {
            let diagnostic_context =
                DiagnosticErrorContext::new("agent_run.launch_commit", "pending_frame_create");
            diag_error!(Error, Subsystem::AgentRun,
                context = &diagnostic_context,
                error = &error,
                session_id = %runtime_session_id,
                turn_id = %turn_id,
                agent_id = %pending_frame.agent_id,
                frame_id = %pending_frame.id,
                frame_revision = pending_frame.revision,
                "Failed to write accepted pending AgentFrame revision"
            );
            return Err(connector_error(format!(
                "accepted pending AgentFrame revision 写入失败: {error}"
            )));
        }
        diag!(Debug, Subsystem::AgentRun,

            session_id = %runtime_session_id,
            agent_id = %pending_frame.agent_id,
            revision = pending_frame.revision,
            "accepted pending AgentFrame revision 已写入"
        );
        outcome.frame_id = Some(pending_frame.id);
        outcome.agent_id = Some(pending_frame.agent_id);
        outcome.wrote_frame_revision = true;

        outcome.bound_current_delivery = self
            .bind_current_delivery_with_anchor(
                agent_repo,
                delivery_binding_repo,
                &anchor,
                pending_frame.agent_id,
                pending_frame.id,
                turn_id,
            )
            .await?;
        self.advance_lifecycle_started(lifecycle_advance, runtime_session_id, turn_id)
            .await?;
        if self
            .sync_hook_runtime_target(runtime_session_id, turn_id, pending_frame.id)
            .await
        {
            outcome.synced_hook_runtime_target = true;
        }
        Ok(outcome)
    }

    async fn commit_revision_from_current_frame(
        &self,
        context: CurrentFrameLaunchCommitContext<'_>,
    ) -> Result<AcceptedLaunchCommitOutcome, ConnectorError> {
        let CurrentFrameLaunchCommitContext {
            frame_repo,
            anchor_repo,
            agent_repo,
            delivery_binding_repo,
            lifecycle_advance,
            runtime_session_id,
            turn_id,
            accepted_capability_state,
        } = context;
        let (anchor, current_frame) = match resolve_current_agent_frame_for_runtime_session(
            runtime_session_id,
            anchor_repo,
            agent_repo,
            frame_repo,
        )
        .await
        {
            Ok(Some((anchor, _agent, current_frame))) => (anchor, current_frame),
            Ok(None) => {
                return Err(connector_error(format!(
                    "accepted launch commit 缺少 RuntimeSession anchor/current AgentFrame: session_id={runtime_session_id}, turn_id={turn_id}"
                )));
            }
            Err(error) => {
                let diagnostic_context =
                    DiagnosticErrorContext::new("agent_run.launch_commit", "resolve_current_frame");
                diag_error!(Error, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &error,
                    session_id = %runtime_session_id,
                    turn_id = %turn_id,
                    "Failed to resolve AgentFrame for accepted launch commit"
                );
                return Err(connector_error(format!(
                    "查找 session 关联的 AgentFrame 失败: {error}"
                )));
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
                outcome.bound_current_delivery = self
                    .bind_current_delivery_with_anchor(
                        agent_repo,
                        delivery_binding_repo,
                        &anchor,
                        frame.agent_id,
                        frame.id,
                        turn_id,
                    )
                    .await?;
                self.advance_lifecycle_started(lifecycle_advance, runtime_session_id, turn_id)
                    .await?;
                if self
                    .sync_hook_runtime_target(runtime_session_id, turn_id, frame.id)
                    .await
                {
                    outcome.synced_hook_runtime_target = true;
                }
            }
            Err(error) => {
                let diagnostic_context = DiagnosticErrorContext::new(
                    "agent_run.launch_commit",
                    "current_frame_revision",
                );
                diag_error!(Error, Subsystem::AgentRun,
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
                return Err(connector_error(format!(
                    "accepted AgentFrame revision 写入失败: {error}"
                )));
            }
        }
        Ok(outcome)
    }

    async fn bind_current_delivery_with_anchor(
        &self,
        agent_repo: &dyn LifecycleAgentRepository,
        delivery_binding_repo: &dyn AgentRunDeliveryBindingRepository,
        anchor: &RuntimeSessionExecutionAnchor,
        agent_id: Uuid,
        frame_id: Uuid,
        turn_id: &str,
    ) -> Result<bool, ConnectorError> {
        if anchor.agent_id != agent_id {
            return Err(connector_error(format!(
                "accepted current delivery agent_id 与 RuntimeSession anchor 不一致: session_id={}, anchor_agent_id={}, frame_agent_id={agent_id}",
                anchor.runtime_session_id, anchor.agent_id
            )));
        }
        let binding = AgentRunDeliveryBinding::from_anchor(
            anchor,
            DeliveryBindingStatus::Running,
            chrono::Utc::now(),
        )
        .mark_running(turn_id, chrono::Utc::now());
        if let Err(error) = delivery_binding_repo.upsert(&binding).await {
            let diagnostic_context =
                DiagnosticErrorContext::new("agent_run.launch_commit", "bind_current_delivery");
            diag_error!(Error, Subsystem::AgentRun,
                context = &diagnostic_context,
                error = &error,
                session_id = %anchor.runtime_session_id,
                run_id = %anchor.run_id,
                agent_id = %agent_id,
                launch_frame_id = %anchor.launch_frame_id,
                "Failed to sync accepted current delivery"
            );
            return Err(connector_error(format!(
                "同步 accepted current delivery 失败: {error}"
            )));
        }
        if let Some(port) = self.project_projection_notifications.as_ref() {
            if let Ok(Some(agent)) = agent_repo.get(agent_id).await {
                let _ = port
                    .publish_project_projection_invalidated(
                        ProjectProjectionInvalidation::agent_run_list(
                            agent.project_id,
                            anchor.run_id,
                            agent_id,
                            Some(frame_id),
                            ControlPlaneProjectionChangeReason::AgentRunShellChanged,
                            Some(anchor.runtime_session_id.clone()),
                        ),
                    )
                    .await;
            }
        }
        Ok(true)
    }

    async fn load_anchor_for_session(
        &self,
        anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
        runtime_session_id: &str,
        turn_id: &str,
    ) -> Result<RuntimeSessionExecutionAnchor, ConnectorError> {
        anchor_repo
            .find_by_session(runtime_session_id)
            .await
            .map_err(|error| {
                let diagnostic_context =
                    DiagnosticErrorContext::new("agent_run.launch_commit", "load_anchor");
                diag_error!(Error, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &error,
                    session_id = %runtime_session_id,
                    turn_id = %turn_id,
                    "Failed to query RuntimeSession anchor for accepted launch commit"
                );
                connector_error(format!(
                    "accepted launch commit 查询 RuntimeSession anchor 失败: {error}"
                ))
            })?
            .ok_or_else(|| {
                connector_error(format!(
                    "accepted launch commit 缺少 RuntimeSession anchor: session_id={runtime_session_id}, turn_id={turn_id}"
                ))
            })
    }

    async fn advance_lifecycle_started(
        &self,
        lifecycle_advance: &dyn AcceptedTurnLifecycleAdvancePort,
        runtime_session_id: &str,
        turn_id: &str,
    ) -> Result<(), ConnectorError> {
        lifecycle_advance
            .advance_node_started_for_accepted_turn(AcceptedTurnLifecycleAdvanceInput {
                runtime_session_id: runtime_session_id.to_string(),
                turn_id: turn_id.to_string(),
            })
            .await
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
    ) -> Result<AcceptedLaunchCommitOutcome, ConnectorError> {
        AgentRunAcceptedLaunchCommitAdapter::commit_accepted_launch(self, input).await
    }
}

pub fn accepted_launch_commit_port(
    frame_repo: Option<Arc<dyn AgentFrameRepository>>,
    anchor_repo: Option<Arc<dyn RuntimeSessionExecutionAnchorRepository>>,
    delivery_binding_repo: Option<Arc<dyn AgentRunDeliveryBindingRepository>>,
    agent_repo: Option<Arc<dyn LifecycleAgentRepository>>,
    lifecycle_advance: Option<Arc<dyn AcceptedTurnLifecycleAdvancePort>>,
    hook_runtime_sync: Option<Arc<dyn AcceptedLaunchHookRuntimeSync>>,
    project_projection_notifications: Option<Arc<dyn ProjectProjectionNotificationPort>>,
) -> Arc<dyn AcceptedLaunchCommitPort> {
    Arc::new(AgentRunAcceptedLaunchCommitAdapter::new(
        AgentRunAcceptedLaunchCommitDeps {
            frame_repo,
            anchor_repo,
            delivery_binding_repo,
            agent_repo,
            lifecycle_advance,
            hook_runtime_sync,
            project_projection_notifications,
        },
    ))
}

fn connector_error(message: impl Into<String>) -> ConnectorError {
    ConnectorError::Runtime(message.into())
}

fn apply_accepted_capability_surface(
    frame: &mut AgentFrame,
    accepted_capability_state: &CapabilityState,
) {
    let surfaces = capability_state_to_frame_surfaces(accepted_capability_state);
    let mut surface = frame.surface_document();
    surface.capability_state = surfaces.effective_capability_json;
    surface.vfs_surface = surfaces.vfs_surface_json;
    surface.mcp_surface = surfaces.mcp_surface_json;
    frame.surface = Some(surface);
    frame.apply_surface_projection();
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
        AgentFrame, AgentFrameSurfaceDocument, AgentSource, LifecycleAgent,
        RuntimeSessionExecutionAnchor,
    };
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use crate::test_support::MemoryAgentRunDeliveryBindingRepository;

    #[derive(Default)]
    struct RecordingProjectProjectionNotificationPort {
        items: Mutex<Vec<ProjectProjectionInvalidation>>,
    }

    #[async_trait]
    impl ProjectProjectionNotificationPort for RecordingProjectProjectionNotificationPort {
        async fn publish_project_projection_invalidated(
            &self,
            invalidation: ProjectProjectionInvalidation,
        ) -> Result<(), String> {
            self.items.lock().unwrap().push(invalidation);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FixtureFrameRepo {
        frames: Mutex<Vec<AgentFrame>>,
        fail_create: AtomicBool,
    }

    #[async_trait]
    impl AgentFrameRepository for FixtureFrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            if self.fail_create.load(Ordering::SeqCst) {
                return Err(DomainError::InvalidConfig(
                    "forced frame create failure".to_string(),
                ));
            }
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
    }

    struct NoopLifecycleAdvance;

    #[async_trait]
    impl AcceptedTurnLifecycleAdvancePort for NoopLifecycleAdvance {
        async fn advance_node_started_for_accepted_turn(
            &self,
            _input: AcceptedTurnLifecycleAdvanceInput,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    struct FailingLifecycleAdvance;

    #[async_trait]
    impl AcceptedTurnLifecycleAdvancePort for FailingLifecycleAdvance {
        async fn advance_node_started_for_accepted_turn(
            &self,
            _input: AcceptedTurnLifecycleAdvanceInput,
        ) -> Result<(), ConnectorError> {
            Err(ConnectorError::Runtime(
                "forced lifecycle update failure".to_string(),
            ))
        }
    }

    #[derive(Default)]
    struct FixtureAgentRepo {
        agents: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait]
    impl LifecycleAgentRepository for FixtureAgentRepo {
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
    struct FixtureAnchorRepo {
        anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for FixtureAnchorRepo {
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

    #[test]
    fn accepted_capability_surface_updates_canonical_document_before_projection() {
        let mut frame = AgentFrame::new_revision(Uuid::new_v4(), 2, "test");
        frame.surface = Some(AgentFrameSurfaceDocument {
            capability_state: Some(serde_json::json!({"stale": true})),
            context_slice: Some(serde_json::json!({"keep": "context"})),
            vfs_surface: Some(serde_json::json!({"stale": "vfs"})),
            mcp_surface: Some(serde_json::json!([{"stale": "mcp"}])),
            ..Default::default()
        });
        frame.effective_capability_json = Some(serde_json::json!({"split": "stale"}));

        let accepted_state = CapabilityState::default();
        let expected = capability_state_to_frame_surfaces(&accepted_state);

        apply_accepted_capability_surface(&mut frame, &accepted_state);

        let surface = frame.surface.as_ref().expect("canonical surface");
        assert_eq!(surface.capability_state, expected.effective_capability_json);
        assert_eq!(surface.vfs_surface, expected.vfs_surface_json);
        assert_eq!(surface.mcp_surface, expected.mcp_surface_json);
        assert_eq!(
            surface.context_slice,
            Some(serde_json::json!({"keep": "context"}))
        );
        assert_eq!(frame.effective_capability_json, surface.capability_state);
        assert_eq!(frame.vfs_surface_json, surface.vfs_surface);
        assert_eq!(frame.mcp_surface_json, surface.mcp_surface);
    }

    #[tokio::test]
    async fn accepted_launch_commit_writes_frame_and_binds_current_delivery() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent);
        let launch_frame = AgentFrame::new_initial(agent.id);
        let pending_frame = AgentFrame::new_revision(agent.id, 2, "test");
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-a",
            run_id,
            launch_frame.id,
            agent.id,
        );

        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo.create(&launch_frame).await.unwrap();
        let agent_repo = Arc::new(FixtureAgentRepo::default());
        agent_repo.create(&agent).await.unwrap();
        let anchor_repo = Arc::new(FixtureAnchorRepo::default());
        anchor_repo.create_once(&anchor).await.unwrap();
        let delivery_binding_repo = Arc::new(MemoryAgentRunDeliveryBindingRepository::default());
        let invalidations = Arc::new(RecordingProjectProjectionNotificationPort::default());

        let adapter = AgentRunAcceptedLaunchCommitAdapter::new(AgentRunAcceptedLaunchCommitDeps {
            frame_repo: Some(frame_repo.clone()),
            anchor_repo: Some(anchor_repo),
            delivery_binding_repo: Some(delivery_binding_repo.clone()),
            agent_repo: Some(agent_repo.clone()),
            lifecycle_advance: Some(Arc::new(NoopLifecycleAdvance)),
            hook_runtime_sync: None,
            project_projection_notifications: Some(invalidations.clone()),
        });

        let outcome = adapter
            .commit_accepted_launch(AcceptedLaunchCommitInput {
                runtime_session_id: "runtime-a".to_string(),
                turn_id: "turn-a".to_string(),
                pending_frame: Some(pending_frame.clone()),
                accepted_capability_state: CapabilityState::default(),
            })
            .await
            .expect("accepted launch commit");

        assert!(outcome.wrote_frame_revision);
        assert!(outcome.bound_current_delivery);
        assert_eq!(outcome.frame_id, Some(pending_frame.id));
        let binding = delivery_binding_repo
            .get_current(run_id, agent.id)
            .await
            .unwrap()
            .expect("current delivery");
        assert_eq!(binding.runtime_session_id, "runtime-a");
        assert_eq!(binding.status, DeliveryBindingStatus::Running);
        let recorded = invalidations.items.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].project_id, project_id);
        assert_eq!(recorded[0].run_id, run_id);
        assert_eq!(recorded[0].agent_id, agent.id);
        assert_eq!(recorded[0].frame_id, Some(pending_frame.id));
        assert_eq!(
            recorded[0].reason,
            ControlPlaneProjectionChangeReason::AgentRunShellChanged
        );
        assert_eq!(
            recorded[0].delivery_runtime_session_id.as_deref(),
            Some("runtime-a")
        );
    }

    #[tokio::test]
    async fn accepted_launch_commit_frame_create_failure_returns_error() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent);
        let launch_frame = AgentFrame::new_initial(agent.id);
        let pending_frame = AgentFrame::new_revision(agent.id, 2, "test");
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-frame-fails",
            run_id,
            launch_frame.id,
            agent.id,
        );

        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo.create(&launch_frame).await.unwrap();
        frame_repo.fail_create.store(true, Ordering::SeqCst);
        let agent_repo = Arc::new(FixtureAgentRepo::default());
        agent_repo.create(&agent).await.unwrap();
        let anchor_repo = Arc::new(FixtureAnchorRepo::default());
        anchor_repo.create_once(&anchor).await.unwrap();
        let delivery_binding_repo = Arc::new(MemoryAgentRunDeliveryBindingRepository::default());
        let adapter = AgentRunAcceptedLaunchCommitAdapter::new(AgentRunAcceptedLaunchCommitDeps {
            frame_repo: Some(frame_repo),
            anchor_repo: Some(anchor_repo),
            delivery_binding_repo: Some(delivery_binding_repo.clone()),
            agent_repo: Some(agent_repo),
            lifecycle_advance: Some(Arc::new(NoopLifecycleAdvance)),
            hook_runtime_sync: None,
            project_projection_notifications: None,
        });

        let error = adapter
            .commit_accepted_launch(AcceptedLaunchCommitInput {
                runtime_session_id: "runtime-frame-fails".to_string(),
                turn_id: "turn-frame-fails".to_string(),
                pending_frame: Some(pending_frame),
                accepted_capability_state: CapabilityState::default(),
            })
            .await
            .expect_err("frame create failure must fail accepted commit");

        assert!(error.to_string().contains("AgentFrame revision 写入失败"));
        assert!(
            delivery_binding_repo
                .get_current(run_id, agent.id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn accepted_launch_commit_lifecycle_failure_returns_error() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent);
        let launch_frame = AgentFrame::new_initial(agent.id);
        let pending_frame = AgentFrame::new_revision(agent.id, 2, "test");
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-lifecycle-fails",
            run_id,
            launch_frame.id,
            agent.id,
        );

        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo.create(&launch_frame).await.unwrap();
        let agent_repo = Arc::new(FixtureAgentRepo::default());
        agent_repo.create(&agent).await.unwrap();
        let anchor_repo = Arc::new(FixtureAnchorRepo::default());
        anchor_repo.create_once(&anchor).await.unwrap();
        let delivery_binding_repo = Arc::new(MemoryAgentRunDeliveryBindingRepository::default());
        let adapter = AgentRunAcceptedLaunchCommitAdapter::new(AgentRunAcceptedLaunchCommitDeps {
            frame_repo: Some(frame_repo),
            anchor_repo: Some(anchor_repo),
            delivery_binding_repo: Some(delivery_binding_repo),
            agent_repo: Some(agent_repo),
            lifecycle_advance: Some(Arc::new(FailingLifecycleAdvance)),
            hook_runtime_sync: None,
            project_projection_notifications: None,
        });

        let error = adapter
            .commit_accepted_launch(AcceptedLaunchCommitInput {
                runtime_session_id: "runtime-lifecycle-fails".to_string(),
                turn_id: "turn-lifecycle-fails".to_string(),
                pending_frame: Some(pending_frame),
                accepted_capability_state: CapabilityState::default(),
            })
            .await
            .expect_err("lifecycle update failure must fail accepted commit");

        assert!(
            error
                .to_string()
                .contains("forced lifecycle update failure")
        );
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

        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo.create(&frame).await.unwrap();
        let agent_repo = Arc::new(FixtureAgentRepo::default());
        agent_repo.create(&agent).await.unwrap();
        let anchor_repo = Arc::new(FixtureAnchorRepo::default());
        anchor_repo.create_once(&anchor).await.unwrap();
        let adapter = AgentRunAcceptedLaunchCommitAdapter::new(AgentRunAcceptedLaunchCommitDeps {
            frame_repo: Some(frame_repo),
            anchor_repo: Some(anchor_repo),
            delivery_binding_repo: None,
            agent_repo: Some(agent_repo.clone()),
            lifecycle_advance: None,
            hook_runtime_sync: None,
            project_projection_notifications: None,
        });

        assert!(adapter.agent_needs_bootstrap("runtime-bootstrap").await);
        adapter.mark_agent_bootstrapped("runtime-bootstrap").await;

        let updated = agent_repo.get(agent.id).await.unwrap().unwrap();
        assert!(!updated.needs_bootstrap());
    }
}
