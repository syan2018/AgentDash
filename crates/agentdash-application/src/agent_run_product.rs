use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentFrameSurfaceExt, ConversationModelConfigInput, ConversationModelConfigModel,
    ConversationModelConfigResolver, ConversationModelConfigStatusModel,
};
use agentdash_application_ports::lifecycle_read_model::{
    AgentRunView, LifecycleReadModelQueryPort, LifecycleSubjectAssociationView,
};
use agentdash_application_ports::vfs_surface_runtime::{
    ResolvedVfsSurface, ResolvedVfsSurfaceSource, VfsSurfaceRuntimeProjection,
};
use agentdash_domain::workflow::{AgentFrame, AgentFrameRepository, LifecycleAgent, LifecycleRun};
use serde_json::Value;

use crate::ApplicationError;
use crate::vfs_surface_resolver::VfsSurfaceResolver;

#[derive(Clone)]
pub struct AgentRunProductQuery {
    lifecycle_read_model_query: Arc<dyn LifecycleReadModelQueryPort>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    vfs_surface_resolver: VfsSurfaceResolver,
}

#[derive(Clone)]
pub struct AgentRunProductQueryDeps {
    pub lifecycle_read_model_query: Arc<dyn LifecycleReadModelQueryPort>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub vfs_surface_resolver: VfsSurfaceResolver,
}

pub struct AgentRunProductQueryInput<'a> {
    pub run: &'a LifecycleRun,
    pub agent: &'a LifecycleAgent,
    pub has_runtime_binding: bool,
    pub runtime_projection: &'a dyn VfsSurfaceRuntimeProjection,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductModel {
    pub run_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub shell: AgentRunProductShellModel,
    pub agent: AgentRunView,
    pub current_frame: Option<AgentRunCurrentFrameModel>,
    pub subject_associations: Vec<LifecycleSubjectAssociationView>,
    pub resource_surface: Option<ResolvedVfsSurface>,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductShellModel {
    pub display_title: String,
    pub title_source: String,
    pub lifecycle_status: String,
    pub last_activity_at: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunCurrentFrameModel {
    pub agent_id: String,
    pub frame_id: String,
    pub revision: i32,
    pub capability_surface: Value,
    pub context_slice: Value,
    pub vfs_surface: Value,
    pub mcp_surface: Value,
    pub execution_profile: Option<Value>,
    pub model_config: ConversationModelConfigModel,
}

impl AgentRunProductQuery {
    pub fn new(deps: AgentRunProductQueryDeps) -> Self {
        Self {
            lifecycle_read_model_query: deps.lifecycle_read_model_query,
            frame_repo: deps.frame_repo,
            vfs_surface_resolver: deps.vfs_surface_resolver,
        }
    }

    pub async fn get(
        &self,
        input: AgentRunProductQueryInput<'_>,
    ) -> Result<AgentRunProductModel, ApplicationError> {
        let run_view = self
            .lifecycle_read_model_query
            .lifecycle_run_view(input.run.id)
            .await?;
        let agent_id = input.agent.id.to_string();
        let agent = run_view
            .agents
            .into_iter()
            .find(|agent| agent.agent_ref.agent_id == agent_id)
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "LifecycleRun {} 缺少 LifecycleAgent {} projection",
                    input.run.id, input.agent.id
                ))
            })?;
        let subject_associations = run_view
            .subject_associations
            .into_iter()
            .filter(|association| {
                association.anchor_agent_id.as_deref() == Some(agent_id.as_str())
                    || association.anchor_agent_id.is_none()
            })
            .collect();
        let current_frame = self
            .frame_repo
            .get_current(input.agent.id)
            .await?
            .map(project_current_frame);
        let resource_surface = if current_frame.is_some() && input.has_runtime_binding {
            Some(
                self.vfs_surface_resolver
                    .resolve_surface(
                        input.runtime_projection,
                        &ResolvedVfsSurfaceSource::AgentRun {
                            run_id: input.run.id,
                            agent_id: input.agent.id,
                        },
                    )
                    .await?,
            )
        } else {
            None
        };

        Ok(AgentRunProductModel {
            run_id: input.run.id.to_string(),
            agent_id,
            project_id: input.run.project_id.to_string(),
            shell: AgentRunProductShellModel {
                display_title: input
                    .agent
                    .workspace_title
                    .clone()
                    .unwrap_or_else(|| input.agent.source.as_str().to_string()),
                title_source: input
                    .agent
                    .workspace_title_source
                    .clone()
                    .unwrap_or_else(|| "agent_source".to_string()),
                lifecycle_status: input.agent.status.clone(),
                last_activity_at: input.run.last_activity_at.to_rfc3339(),
            },
            agent,
            current_frame,
            subject_associations,
            resource_surface,
        })
    }
}

fn project_current_frame(frame: AgentFrame) -> AgentRunCurrentFrameModel {
    let execution_profile = frame.typed_execution_profile();
    let model_config = if let Some(execution_profile) = execution_profile.as_ref() {
        ConversationModelConfigResolver::resolve(ConversationModelConfigInput {
            frame_execution_profile: Some(execution_profile),
            ..Default::default()
        })
        .view
    } else {
        ConversationModelConfigModel {
            status: ConversationModelConfigStatusModel::ModelRequired,
            effective_executor_config: None,
            missing_fields: vec!["execution_profile".to_string()],
            message: Some("current AgentFrame 缺少可解析的 execution profile。".to_string()),
        }
    };

    AgentRunCurrentFrameModel {
        agent_id: frame.agent_id.to_string(),
        frame_id: frame.id.to_string(),
        revision: frame.revision,
        capability_surface: frame
            .effective_capability_json
            .unwrap_or(serde_json::Value::Null),
        context_slice: frame.context_slice_json.unwrap_or(serde_json::Value::Null),
        vfs_surface: frame.vfs_surface_json.unwrap_or(serde_json::Value::Null),
        mcp_surface: frame.mcp_surface_json.unwrap_or(serde_json::Value::Null),
        execution_profile: frame.execution_profile_json,
        model_config,
    }
}

#[cfg(test)]
mod tests {
    use agentdash_application_agentrun::agent_run::{
        ConversationModelConfigSourceModel, ConversationModelConfigStatusModel,
    };
    use agentdash_domain::workflow::AgentFrame;
    use serde_json::json;
    use uuid::Uuid;

    use super::project_current_frame;

    #[test]
    fn current_frame_projects_effective_model_config_from_execution_profile() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.execution_profile_json = Some(json!({
            "executor": "PI_AGENT",
            "provider_id": "openai",
            "model_id": "gpt-test"
        }));
        frame.effective_capability_json = Some(json!({ "version": 1 }));

        let model = project_current_frame(frame);

        assert_eq!(
            model.model_config.status,
            ConversationModelConfigStatusModel::Resolved
        );
        let effective = model
            .model_config
            .effective_executor_config
            .expect("effective config");
        assert_eq!(effective.executor, "PI_AGENT");
        assert_eq!(effective.provider_id.as_deref(), Some("openai"));
        assert_eq!(effective.model_id.as_deref(), Some("gpt-test"));
        assert_eq!(
            effective.source,
            ConversationModelConfigSourceModel::FrameExecutionProfile
        );
        assert_eq!(model.capability_surface, json!({ "version": 1 }));
    }

    #[test]
    fn current_frame_reports_missing_cloud_native_model_fields() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.execution_profile_json = Some(json!({ "executor": "PI_AGENT" }));

        let model = project_current_frame(frame);

        assert_eq!(
            model.model_config.status,
            ConversationModelConfigStatusModel::ModelRequired
        );
        assert_eq!(
            model.model_config.missing_fields,
            vec!["provider_id".to_string(), "model_id".to_string()]
        );
    }

    #[test]
    fn current_frame_does_not_invent_default_executor_without_execution_profile() {
        let frame = AgentFrame::new_initial(Uuid::new_v4());

        let model = project_current_frame(frame);

        assert_eq!(
            model.model_config.status,
            ConversationModelConfigStatusModel::ModelRequired
        );
        assert!(model.model_config.effective_executor_config.is_none());
        assert_eq!(
            model.model_config.missing_fields,
            vec!["execution_profile".to_string()]
        );
    }
}
