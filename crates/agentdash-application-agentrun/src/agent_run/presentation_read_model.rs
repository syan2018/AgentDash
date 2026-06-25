use std::{io, sync::Arc};

use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::workflow::{AgentFrame, RuntimeSessionExecutionAnchor};

use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::agent_run::lifecycle_read_model::{
    self as lifecycle_read_model, AgentRunView, LifecycleRunView, LifecycleSubjectAssociationView,
};
use crate::agent_run::{
    AgentRunRuntimeSurfaceQueryError, AgentRunRuntimeSurfaceQueryPort,
    ConversationEffectiveExecutorConfigModel, ConversationModelConfigResolver,
    ConversationModelConfigSourceModel, RuntimeSurfaceQueryPurpose,
};
use crate::agent_run_repository_set::RepositorySet;
use crate::session::{
    ExecutionStatus, SessionCoreService, SessionEventingService, SessionExecutionState, SessionMeta,
};

#[derive(Clone)]
pub struct AgentRunPresentationReadModelQuery {
    repos: RepositorySet,
    session_core: SessionCoreService,
    session_eventing: SessionEventingService,
    surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
}

#[derive(Clone)]
pub struct AgentRunPresentationReadModelQueryDeps {
    pub repos: RepositorySet,
    pub session_core: SessionCoreService,
    pub session_eventing: SessionEventingService,
    pub surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
}

impl AgentRunPresentationReadModelQuery {
    pub fn new(deps: AgentRunPresentationReadModelQueryDeps) -> Self {
        Self {
            repos: deps.repos,
            session_core: deps.session_core,
            session_eventing: deps.session_eventing,
            surface_query: deps.surface_query,
        }
    }

    pub async fn agent_frame_runtime(
        &self,
        frame_id: Uuid,
    ) -> Result<AgentFrameRuntimeReadModel, AgentRunPresentationReadModelError> {
        let frame = self
            .repos
            .agent_frame_repo
            .get(frame_id)
            .await?
            .ok_or(AgentRunPresentationReadModelError::MissingAgentFrame { frame_id })?;
        let agent = self
            .repos
            .lifecycle_agent_repo
            .get(frame.agent_id)
            .await?
            .ok_or(AgentRunPresentationReadModelError::MissingLifecycleAgent {
                agent_id: frame.agent_id,
            })?;
        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(agent.run_id)
            .await?
            .ok_or(AgentRunPresentationReadModelError::MissingLifecycleRun {
                run_id: agent.run_id,
            })?;
        if agent.project_id != run.project_id {
            return Err(AgentRunPresentationReadModelError::ControlPlaneMismatch {
                message: format!(
                    "AgentFrame `{frame_id}` 所属 agent project 与 run project 不一致"
                ),
            });
        }
        self.agent_frame_runtime_from_frame(frame, Some(run.project_id))
            .await
    }

    pub async fn runtime_session_trace(
        &self,
        runtime_session_id: &str,
    ) -> Result<RuntimeSessionTraceReadModel, AgentRunPresentationReadModelError> {
        let frame_ref = match self
            .current_runtime_frame(runtime_session_id, "runtime_session_trace")
            .await
        {
            Ok(Some(frame)) => Some(agent_frame_ref_model(&frame)),
            Ok(None) => None,
            Err(AgentRunPresentationReadModelError::RuntimeSurface(
                AgentRunRuntimeSurfaceQueryError::MissingAnchor { .. },
            )) => None,
            Err(error) => return Err(error),
        };
        let events = self
            .session_eventing
            .list_event_page(runtime_session_id, 0, 200)
            .await?
            .events
            .into_iter()
            .filter_map(|event| serde_json::to_value(event).ok())
            .collect::<Vec<_>>();

        Ok(RuntimeSessionTraceReadModel {
            runtime_session_id: runtime_session_id.to_string(),
            frame_ref,
            events,
            turns: Vec::new(),
        })
    }

    pub async fn session_runtime_control(
        &self,
        runtime_session_id: &str,
    ) -> Result<SessionRuntimeControlReadModel, AgentRunPresentationReadModelError> {
        let session_meta = self
            .session_core
            .get_session_meta(runtime_session_id)
            .await?
            .ok_or_else(|| AgentRunPresentationReadModelError::MissingSession {
                runtime_session_id: runtime_session_id.to_string(),
            })?;
        let Some(anchor) = self
            .repos
            .execution_anchor_repo
            .find_by_session(runtime_session_id)
            .await?
        else {
            return Ok(SessionRuntimeControlReadModel {
                runtime_session_id: runtime_session_id.to_string(),
                session_meta,
                control_plane: SessionRuntimeControlPlaneReadModel {
                    status: SessionRuntimeControlPlaneStatusModel::UnboundTrace,
                    reason: Some(
                        "当前 Session 只有 runtime trace，没有绑定 Agent 控制面。".to_string(),
                    ),
                },
                anchor: None,
                run: None,
                agent: None,
                frame_runtime: None,
                subject_associations: Vec::new(),
                project_id: None,
            });
        };

        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(anchor.run_id)
            .await?
            .ok_or(AgentRunPresentationReadModelError::MissingLifecycleRun {
                run_id: anchor.run_id,
            })?;
        let agent = self
            .repos
            .lifecycle_agent_repo
            .get(anchor.agent_id)
            .await?
            .ok_or(AgentRunPresentationReadModelError::MissingLifecycleAgent {
                agent_id: anchor.agent_id,
            })?;
        if agent.run_id != run.id || agent.project_id != run.project_id {
            return Err(AgentRunPresentationReadModelError::ControlPlaneMismatch {
                message: format!(
                    "runtime session anchor agent 与 run 不一致: {runtime_session_id}"
                ),
            });
        }

        let frame_runtime = match self
            .current_runtime_frame(runtime_session_id, "session_runtime_control")
            .await
        {
            Ok(Some(frame)) => Some(
                self.agent_frame_runtime_from_frame(frame, Some(run.project_id))
                    .await?,
            ),
            Ok(None) => None,
            Err(AgentRunPresentationReadModelError::RuntimeSurface(
                AgentRunRuntimeSurfaceQueryError::MissingCurrentFrame { .. },
            )) => None,
            Err(error) => return Err(error),
        };
        let run_view = lifecycle_read_model::build_lifecycle_run_view(&self.repos, &run).await?;
        let agent_view = run_view
            .agents
            .iter()
            .find(|view| view.agent_ref.agent_id == agent.id.to_string())
            .cloned();
        let agent_id_string = agent.id.to_string();
        let subject_associations = run_view
            .subject_associations
            .iter()
            .filter(|assoc| {
                assoc.anchor_agent_id.as_deref() == Some(agent_id_string.as_str())
                    || assoc.anchor_agent_id.is_none()
            })
            .cloned()
            .collect::<Vec<_>>();
        let execution_state = self
            .session_core
            .inspect_session_execution_state(runtime_session_id)
            .await?;
        let delivery_running = session_meta.last_delivery_status == ExecutionStatus::Running
            || matches!(execution_state, SessionExecutionState::Running { .. });
        let delivery_cancelling =
            matches!(execution_state, SessionExecutionState::Cancelling { .. });
        let terminal_agent = is_terminal_agent_status(&agent.status);
        let has_frame = frame_runtime.is_some();
        let control_plane = if terminal_agent {
            SessionRuntimeControlPlaneReadModel {
                status: SessionRuntimeControlPlaneStatusModel::Terminal,
                reason: Some("当前 Agent 已结束。".to_string()),
            }
        } else if !has_frame {
            SessionRuntimeControlPlaneReadModel {
                status: SessionRuntimeControlPlaneStatusModel::FrameMissing,
                reason: Some("当前 Agent 没有可投递的 runtime frame。".to_string()),
            }
        } else if delivery_cancelling {
            SessionRuntimeControlPlaneReadModel {
                status: SessionRuntimeControlPlaneStatusModel::AnchoredCancelling,
                reason: Some("当前 Session 正在取消中，等待执行器收口。".to_string()),
            }
        } else if delivery_running {
            SessionRuntimeControlPlaneReadModel {
                status: SessionRuntimeControlPlaneStatusModel::AnchoredRunning,
                reason: Some("当前 Session 正在执行中。".to_string()),
            }
        } else {
            SessionRuntimeControlPlaneReadModel {
                status: SessionRuntimeControlPlaneStatusModel::AnchoredIdle,
                reason: None,
            }
        };

        Ok(SessionRuntimeControlReadModel {
            runtime_session_id: runtime_session_id.to_string(),
            session_meta,
            control_plane,
            anchor: Some(anchor),
            run: Some(run_view),
            agent: agent_view,
            frame_runtime,
            subject_associations,
            project_id: Some(run.project_id),
        })
    }

    async fn current_runtime_frame(
        &self,
        runtime_session_id: &str,
        component: &'static str,
    ) -> Result<Option<AgentFrame>, AgentRunPresentationReadModelError> {
        let surface = self
            .surface_query
            .current_runtime_surface(
                runtime_session_id,
                RuntimeSurfaceQueryPurpose::new(component),
            )
            .await?;
        let frame = self
            .repos
            .agent_frame_repo
            .get(surface.current_surface_frame_id)
            .await?
            .ok_or(AgentRunPresentationReadModelError::MissingAgentFrame {
                frame_id: surface.current_surface_frame_id,
            })?;
        if frame.agent_id != surface.agent_id {
            return Err(AgentRunPresentationReadModelError::ControlPlaneMismatch {
                message: format!(
                    "current surface frame agent 与 runtime surface agent 不一致: {runtime_session_id}"
                ),
            });
        }
        Ok(Some(frame))
    }

    async fn agent_frame_runtime_from_frame(
        &self,
        frame: AgentFrame,
        known_project_id: Option<Uuid>,
    ) -> Result<AgentFrameRuntimeReadModel, AgentRunPresentationReadModelError> {
        let project_id = match known_project_id {
            Some(project_id) => project_id,
            None => {
                let agent = self
                    .repos
                    .lifecycle_agent_repo
                    .get(frame.agent_id)
                    .await?
                    .ok_or(AgentRunPresentationReadModelError::MissingLifecycleAgent {
                        agent_id: frame.agent_id,
                    })?;
                agent.project_id
            }
        };
        let runtime_session_refs = self
            .repos
            .execution_anchor_repo
            .list_by_agent(frame.agent_id)
            .await?
            .into_iter()
            .map(|anchor| RuntimeSessionRefReadModel {
                runtime_session_id: anchor.runtime_session_id,
            })
            .collect();
        Ok(AgentFrameRuntimeReadModel {
            project_id,
            frame_ref: agent_frame_ref_model(&frame),
            capability_surface: frame
                .effective_capability_json
                .clone()
                .unwrap_or(Value::Null),
            context_slice: frame.context_slice_json.clone().unwrap_or(Value::Null),
            vfs_surface: frame.vfs_surface_json.clone().unwrap_or(Value::Null),
            mcp_surface: frame.mcp_surface_json.clone().unwrap_or(Value::Null),
            runtime_session_refs,
            execution_profile: frame.execution_profile_json.clone(),
            effective_executor_config: frame.typed_execution_profile().map(|config| {
                ConversationModelConfigResolver::view_for_config(
                    &config,
                    ConversationModelConfigSourceModel::FrameExecutionProfile,
                )
            }),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentFrameRuntimeReadModel {
    pub project_id: Uuid,
    pub frame_ref: AgentFrameRefReadModel,
    pub capability_surface: Value,
    pub context_slice: Value,
    pub vfs_surface: Value,
    pub mcp_surface: Value,
    pub runtime_session_refs: Vec<RuntimeSessionRefReadModel>,
    pub execution_profile: Option<Value>,
    pub effective_executor_config: Option<ConversationEffectiveExecutorConfigModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFrameRefReadModel {
    pub agent_id: String,
    pub frame_id: String,
    pub revision: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionRefReadModel {
    pub runtime_session_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSessionTraceReadModel {
    pub runtime_session_id: String,
    pub frame_ref: Option<AgentFrameRefReadModel>,
    pub events: Vec<Value>,
    pub turns: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct SessionRuntimeControlReadModel {
    pub runtime_session_id: String,
    pub session_meta: SessionMeta,
    pub control_plane: SessionRuntimeControlPlaneReadModel,
    pub anchor: Option<RuntimeSessionExecutionAnchor>,
    pub run: Option<LifecycleRunView>,
    pub agent: Option<AgentRunView>,
    pub frame_runtime: Option<AgentFrameRuntimeReadModel>,
    pub subject_associations: Vec<LifecycleSubjectAssociationView>,
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRuntimeControlPlaneReadModel {
    pub status: SessionRuntimeControlPlaneStatusModel,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRuntimeControlPlaneStatusModel {
    UnboundTrace,
    AnchoredIdle,
    AnchoredRunning,
    AnchoredCancelling,
    Terminal,
    FrameMissing,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentRunPresentationReadModelError {
    #[error("session 不存在: {runtime_session_id}")]
    MissingSession { runtime_session_id: String },
    #[error("lifecycle_run 不存在: {run_id}")]
    MissingLifecycleRun { run_id: Uuid },
    #[error("lifecycle_agent 不存在: {agent_id}")]
    MissingLifecycleAgent { agent_id: Uuid },
    #[error("agent_frame 不存在: {frame_id}")]
    MissingAgentFrame { frame_id: Uuid },
    #[error("presentation read model 控制面不一致: {message}")]
    ControlPlaneMismatch { message: String },
    #[error("{0}")]
    RuntimeSurface(#[from] AgentRunRuntimeSurfaceQueryError),
    #[error("{0}")]
    Domain(#[from] DomainError),
    #[error("{0}")]
    SessionStore(#[from] crate::session::persistence::SessionStoreError),
    #[error("{0}")]
    Io(#[from] io::Error),
}

fn agent_frame_ref_model(frame: &AgentFrame) -> AgentFrameRefReadModel {
    AgentFrameRefReadModel {
        agent_id: frame.agent_id.to_string(),
        frame_id: frame.id.to_string(),
        revision: Some(frame.revision),
    }
}

fn is_terminal_agent_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}
