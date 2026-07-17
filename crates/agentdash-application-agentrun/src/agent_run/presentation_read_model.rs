use std::{io, sync::Arc};

use serde_json::Value;
use uuid::Uuid;

use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository;
use agentdash_domain::DomainError;
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, LifecycleAgentRepository, LifecycleRunRepository,
};

use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::agent_run::runtime_session_boundary::{SessionEventingService, SessionStoreError};
use crate::agent_run::{
    AgentRunRuntimeSurfaceQueryError, AgentRunRuntimeSurfaceQueryPort,
    ConversationEffectiveExecutorConfigModel, ConversationModelConfigResolver,
    ConversationModelConfigSourceModel, RuntimeSurfaceQueryPurpose,
};
use crate::error::WorkflowApplicationError;

#[derive(Clone)]
pub struct AgentRunPresentationReadModelQuery {
    repos: AgentRunPresentationReadModelQueryRepos,
    session_eventing: SessionEventingService,
    surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
}

#[derive(Clone)]
pub struct AgentRunPresentationReadModelQueryRepos {
    pub agent_frame_repo: Arc<dyn AgentFrameRepository>,
    pub lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
}

#[derive(Clone)]
pub struct AgentRunPresentationReadModelQueryDeps {
    pub repos: AgentRunPresentationReadModelQueryRepos,
    pub session_eventing: SessionEventingService,
    pub surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
}

impl AgentRunPresentationReadModelQuery {
    pub fn new(deps: AgentRunPresentationReadModelQueryDeps) -> Self {
        Self {
            repos: deps.repos,
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
            .runtime_binding_repo
            .list_by_agent(frame.agent_id)
            .await
            .map_err(|error| {
                AgentRunPresentationReadModelError::Application(WorkflowApplicationError::Internal(
                    error.to_string(),
                ))
            })?
            .into_iter()
            .map(|binding| RuntimeSessionRefReadModel {
                runtime_session_id: binding.thread_id.to_string(),
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
    Application(#[from] WorkflowApplicationError),
    #[error("{0}")]
    Domain(#[from] DomainError),
    #[error("{0}")]
    SessionStore(#[from] SessionStoreError),
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
