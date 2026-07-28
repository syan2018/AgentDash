use agentdash_application_vfs::{
    ResolvedVfsSurface, ResolvedVfsSurfaceSource, VfsSurfaceRuntimeProjection,
    build_surface_summary,
};
use agentdash_domain::agent::{ProjectAgent, ProjectAgentRepository};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, LifecycleAgent, LifecycleGateRepository, LifecycleRun,
    LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository,
};
use agentdash_platform_spi::Vfs;

use crate::agent_run::lifecycle_read_model_facade::{
    AgentRunRefView, AgentRunView, LifecycleSubjectAssociationView, RuntimeThreadRefView,
    SubjectRefView,
};
use crate::agent_run::{
    AgentConversationSnapshotInput, AgentConversationSnapshotResolver, AgentRunOwnershipModel,
    AgentRunProductProjectionQueryPort, AgentRunProductRuntimeBinding,
    ConversationModelConfigInput, ConversationModelConfigResolver,
    ConversationModelConfigSourceModel, ConversationWaitingItemModel, ValidationSeverityModel,
    resolve_agent_run_display_title,
};
use crate::error::WorkflowApplicationError;

use super::state::derive_workspace_delivery_status;
use super::types::{
    AgentRunListItem, AgentRunResourceSurfaceCoordinateModel,
    AgentRunResourceSurfaceSourceAnchorModel, AgentRunWorkspaceFrameRefModel,
    AgentRunWorkspaceFrameRuntimeModel, AgentRunWorkspaceQueryInput, AgentRunWorkspaceShellModel,
    AgentRunWorkspaceSnapshot, SubjectRefModel,
};

#[derive(Clone, Copy)]
pub struct AgentRunWorkspaceQueryDeps<'a> {
    pub product_projection: &'a dyn AgentRunProductProjectionQueryPort,
    pub agent_frame_repo: &'a dyn AgentFrameRepository,
    pub project_agent_repo: &'a dyn ProjectAgentRepository,
    pub lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    pub lifecycle_gate_repo: &'a dyn LifecycleGateRepository,
    pub inline_file_repo: &'a dyn InlineFileRepository,
}

pub struct AgentRunWorkspaceQueryService<'a> {
    repos: AgentRunWorkspaceQueryDeps<'a>,
    vfs_runtime: &'a dyn VfsSurfaceRuntimeProjection,
}

impl<'a> AgentRunWorkspaceQueryService<'a> {
    pub fn new(
        repos: AgentRunWorkspaceQueryDeps<'a>,
        vfs_runtime: &'a dyn VfsSurfaceRuntimeProjection,
    ) -> Self {
        Self { repos, vfs_runtime }
    }

    pub async fn resolve(
        &self,
        input: AgentRunWorkspaceQueryInput,
    ) -> Result<AgentRunWorkspaceSnapshot, WorkflowApplicationError> {
        let viewer_user_id = input.viewer_user_id;
        let execution_authority_vfs = input.execution_authority_vfs;
        let run = input.run;
        let agent = input.agent;
        let target = AgentRunTarget {
            run_id: run.id,
            agent_id: agent.id,
        };
        let ownership = AgentRunOwnershipModel::from_owner_fields(
            run.created_by_user_id.clone(),
            agent.created_by_user_id.clone(),
            viewer_user_id.as_deref(),
        );

        let binding = self.product_runtime_binding(&target).await?;
        let binding = binding.as_ref();
        let runtime_thread_id = binding.map(|binding| binding.runtime_thread_id.to_string());
        let delivery_status = derive_workspace_delivery_status(&agent.status);
        let frame = self.resolve_frame(&agent, binding).await?;
        let frame_ref = frame.as_ref().map(|frame| (frame.id, frame.revision));
        let frame_execution_profile = frame.as_ref().and_then(|frame| {
            crate::agent_run::AgentFrameSurfaceExt::typed_execution_profile(frame)
        });
        let frame_runtime = frame.as_ref().map(|frame| {
            frame_runtime_model(
                frame,
                runtime_thread_id
                    .as_ref()
                    .map(|runtime_thread_id| {
                        vec![RuntimeThreadRefView {
                            runtime_thread_id: runtime_thread_id.clone(),
                        }]
                    })
                    .unwrap_or_default(),
            )
        });

        let resource_surface = self
            .resolve_resource_surface(
                &target,
                frame.as_ref(),
                binding.is_some(),
                execution_authority_vfs,
            )
            .await?;
        let resource_surface_coordinate = match (frame.as_ref(), binding) {
            (Some(frame), Some(binding)) => Some(resource_surface_coordinate_model(
                frame,
                binding,
                &delivery_status,
            )),
            (Some(frame), _) => Some(AgentRunResourceSurfaceCoordinateModel {
                surface_frame_ref: frame_ref_model(frame),
                source_anchor: None,
            }),
            _ => None,
        };

        let subject_associations = self.subject_associations(run.id, agent.id).await?;
        let agent_view = Some(agent_view(&run, &agent, &delivery_status));
        let open_wait_items = self
            .repos
            .lifecycle_gate_repo
            .list_open_for_agent(agent.id)
            .await
            .map_err(WorkflowApplicationError::from)?
            .into_iter()
            .map(|gate| ConversationWaitingItemModel::from_lifecycle_gate(&gate))
            .collect::<Vec<_>>();

        let project_agent = self.load_project_agent(&run, &agent).await?;
        let project_agent_preset_config = project_agent
            .as_ref()
            .map(|project_agent| {
                project_agent
                    .preset_config()
                    .map(|preset| preset.to_agent_config(&project_agent.agent_type))
            })
            .transpose()
            .map_err(WorkflowApplicationError::from)?;
        let model_config = ConversationModelConfigResolver::resolve(ConversationModelConfigInput {
            project_agent_preset: project_agent_preset_config.as_ref(),
            frame_execution_profile: frame_execution_profile.as_ref(),
            ..Default::default()
        })
        .view;
        let resource_diagnostics =
            workspace_resource_diagnostics(run.id, resource_surface.as_ref());
        let conversation =
            AgentConversationSnapshotResolver::resolve(AgentConversationSnapshotInput {
                project_id: run.project_id,
                run_id: run.id,
                agent_id: agent.id,
                frame_ref,
                subject_associations: subject_associations.clone(),
                open_wait_items,
                resource_surface: resource_surface.clone(),
                resource_surface_coordinate: resource_surface_coordinate.clone(),
                resource_diagnostics,
                model_config,
                ownership: ownership.clone(),
            });
        let shell = shell_model(&agent, &delivery_status);

        Ok(AgentRunWorkspaceSnapshot {
            run,
            agent,
            ownership,
            shell,
            runtime_thread_id,
            agent_view,
            frame_runtime,
            subject_associations,
            resource_surface,
            resource_surface_coordinate,
            conversation,
        })
    }

    pub async fn resolve_list_item(
        &self,
        input: AgentRunWorkspaceQueryInput,
    ) -> Result<AgentRunListItem, WorkflowApplicationError> {
        let run = input.run;
        let agent = input.agent;
        let target = AgentRunTarget {
            run_id: run.id,
            agent_id: agent.id,
        };
        let binding = self.product_runtime_binding(&target).await?;
        let runtime_thread_id = binding
            .as_ref()
            .map(|binding| binding.runtime_thread_id.to_string());
        let delivery_status = derive_workspace_delivery_status(&agent.status);
        let shell = shell_model(&agent, &delivery_status);
        let project_agent = self.load_project_agent(&run, &agent).await?;
        let association = self
            .repos
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, Some(agent.id))
            .await
            .map_err(WorkflowApplicationError::from)?
            .into_iter()
            .next();
        let subject_ref = association.as_ref().map(|association| SubjectRefModel {
            kind: association.subject_kind.clone(),
            id: association.subject_id.to_string(),
        });
        let subject_label = association.as_ref().and_then(|association| {
            subject_label_from_metadata(association.metadata_json.as_ref())
        });

        Ok(AgentRunListItem {
            run,
            agent,
            shell,
            project_agent_label: project_agent.as_ref().map(project_agent_display_label),
            runtime_thread_id,
            subject_ref,
            subject_label,
        })
    }

    async fn product_runtime_binding(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, WorkflowApplicationError> {
        self.repos
            .product_projection
            .runtime_product_binding(target)
            .await
            .map_err(|error| WorkflowApplicationError::Conflict(error.to_string()))
    }

    async fn resolve_frame(
        &self,
        agent: &LifecycleAgent,
        binding: Option<&AgentRunProductRuntimeBinding>,
    ) -> Result<Option<AgentFrame>, WorkflowApplicationError> {
        let frame = match binding {
            Some(binding) => self
                .repos
                .agent_frame_repo
                .get(binding.launch_frame.frame_id)
                .await
                .map_err(WorkflowApplicationError::from)?,
            None => self
                .repos
                .agent_frame_repo
                .get_latest(agent.id)
                .await
                .map_err(WorkflowApplicationError::from)?,
        };
        if let (Some(frame), Some(binding)) = (&frame, binding)
            && (frame.agent_id != binding.launch_frame.agent_id
                || frame.agent_id != agent.id
                || u64::try_from(frame.revision).ok() != Some(binding.launch_frame.revision))
        {
            return Err(WorkflowApplicationError::Conflict(
                "AgentRun Product binding 固定的 AgentFrame revision 与持久化事实不一致"
                    .to_string(),
            ));
        }
        Ok(frame)
    }

    async fn resolve_resource_surface(
        &self,
        target: &AgentRunTarget,
        frame: Option<&AgentFrame>,
        has_product_binding: bool,
        execution_authority_vfs: Option<Vfs>,
    ) -> Result<Option<ResolvedVfsSurface>, WorkflowApplicationError> {
        let vfs = if has_product_binding {
            execution_authority_vfs.ok_or_else(|| {
                WorkflowApplicationError::Conflict(
                    "AgentRun current workspace 缺少 ExecutionAuthority VFS".to_string(),
                )
            })?
        } else {
            let Some(frame) = frame else {
                return Ok(None);
            };
            crate::agent_run::AgentFrameSurfaceExt::typed_vfs(frame).unwrap_or_default()
        };
        let source = ResolvedVfsSurfaceSource::AgentRun {
            run_id: target.run_id,
            agent_id: target.agent_id,
        };
        Ok(Some(
            build_surface_summary(self.repos.inline_file_repo, self.vfs_runtime, &source, &vfs)
                .await,
        ))
    }

    async fn subject_associations(
        &self,
        run_id: uuid::Uuid,
        agent_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleSubjectAssociationView>, WorkflowApplicationError> {
        self.repos
            .lifecycle_subject_association_repo
            .list_by_anchor(run_id, Some(agent_id))
            .await
            .map_err(WorkflowApplicationError::from)
            .map(|associations| {
                associations
                    .into_iter()
                    .filter(|association| {
                        association.anchor_agent_id.is_none()
                            || association.anchor_agent_id == Some(agent_id)
                    })
                    .map(subject_association_view)
                    .collect()
            })
    }

    async fn load_project_agent(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
    ) -> Result<Option<ProjectAgent>, WorkflowApplicationError> {
        let Some(project_agent_id) = agent.project_agent_id else {
            return Ok(None);
        };
        self.repos
            .project_agent_repo
            .get_by_project_and_id(run.project_id, project_agent_id)
            .await
            .map_err(WorkflowApplicationError::from)
    }
}

fn agent_view(
    run: &LifecycleRun,
    agent: &LifecycleAgent,
    last_delivery_status: &str,
) -> AgentRunView {
    AgentRunView {
        agent_ref: AgentRunRefView {
            run_id: run.id.to_string(),
            agent_id: agent.id.to_string(),
        },
        project_id: run.project_id.to_string(),
        source: agent.source.as_str().to_string(),
        project_agent_id: agent.project_agent_id.map(|id| id.to_string()),
        status: agent.status.clone(),
        last_delivery_status: Some(last_delivery_status.to_string()),
        created_at: agent.created_at.to_rfc3339(),
        updated_at: agent.updated_at.to_rfc3339(),
    }
}

fn subject_association_view(
    association: LifecycleSubjectAssociation,
) -> LifecycleSubjectAssociationView {
    LifecycleSubjectAssociationView {
        id: association.id.to_string(),
        anchor_run_id: association.anchor_run_id.to_string(),
        anchor_agent_id: association.anchor_agent_id.map(|id| id.to_string()),
        subject_ref: SubjectRefView {
            kind: association.subject_kind,
            id: association.subject_id.to_string(),
        },
        role: association.role,
        metadata: association.metadata_json,
        created_at: association.created_at.to_rfc3339(),
    }
}

fn project_agent_display_label(project_agent: &ProjectAgent) -> String {
    project_agent
        .preset_config()
        .ok()
        .and_then(|preset| preset.display_name)
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| project_agent.name.clone())
}

fn shell_model(agent: &LifecycleAgent, delivery_status: &str) -> AgentRunWorkspaceShellModel {
    let title = resolve_agent_run_display_title(
        agent.workspace_title.as_deref(),
        agent.workspace_title_source.as_deref(),
    );
    AgentRunWorkspaceShellModel {
        display_title: title.value,
        title_source: title.source,
        delivery_status: delivery_status.to_owned(),
        last_activity_at: agent.updated_at.to_rfc3339(),
    }
}

fn subject_label_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    let metadata = metadata?;
    ["label", "title", "name"]
        .iter()
        .find_map(|key| metadata.get(key).and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn frame_runtime_model(
    frame: &AgentFrame,
    runtime_thread_refs: Vec<RuntimeThreadRefView>,
) -> AgentRunWorkspaceFrameRuntimeModel {
    AgentRunWorkspaceFrameRuntimeModel {
        frame_ref: frame_ref_model(frame),
        capability_surface: frame
            .effective_capability_json
            .clone()
            .unwrap_or(serde_json::Value::Null),
        context_slice: frame
            .context_slice_json
            .clone()
            .unwrap_or(serde_json::Value::Null),
        vfs_surface: frame
            .vfs_surface_json
            .clone()
            .unwrap_or(serde_json::Value::Null),
        mcp_surface: frame
            .mcp_surface_json
            .clone()
            .unwrap_or(serde_json::Value::Null),
        runtime_thread_refs,
        execution_profile: frame.execution_profile_json.clone(),
        effective_executor_config: crate::agent_run::AgentFrameSurfaceExt::typed_execution_profile(
            frame,
        )
        .map(|config| {
            ConversationModelConfigResolver::view_for_config(
                &config,
                ConversationModelConfigSourceModel::FrameExecutionProfile,
            )
        }),
    }
}

fn frame_ref_model(frame: &AgentFrame) -> AgentRunWorkspaceFrameRefModel {
    AgentRunWorkspaceFrameRefModel {
        agent_id: frame.agent_id.to_string(),
        frame_id: frame.id.to_string(),
        revision: Some(frame.revision),
    }
}

fn resource_surface_coordinate_model(
    frame: &AgentFrame,
    binding: &AgentRunProductRuntimeBinding,
    delivery_status: &str,
) -> AgentRunResourceSurfaceCoordinateModel {
    let observed_at = frame.created_at.to_rfc3339();
    AgentRunResourceSurfaceCoordinateModel {
        surface_frame_ref: frame_ref_model(frame),
        source_anchor: Some(AgentRunResourceSurfaceSourceAnchorModel {
            runtime_thread_id: binding.runtime_thread_id.to_string(),
            launch_frame_id: binding.launch_frame.frame_id.to_string(),
            orchestration_id: None,
            node_path: None,
            node_attempt: None,
            delivery_status: delivery_status.to_owned(),
            observed_at,
        }),
    }
}

fn workspace_resource_diagnostics(
    run_id: uuid::Uuid,
    resource_surface: Option<&ResolvedVfsSurface>,
) -> Vec<crate::agent_run::ConversationDiagnosticModel> {
    let has_lifecycle_mount = resource_surface
        .map(|surface| {
            surface
                .mounts
                .iter()
                .any(|mount| mount.id == "lifecycle" && mount.provider == "lifecycle_vfs")
        })
        .unwrap_or(false);
    if has_lifecycle_mount {
        return Vec::new();
    }
    vec![crate::agent_run::ConversationDiagnosticModel {
        code: "resource_surface_lifecycle_mount_missing".to_string(),
        severity: ValidationSeverityModel::Error,
        message: "当前 AgentRun workspace resource_surface 缺少 lifecycle_vfs mount。".to_string(),
        detail: Some(serde_json::json!({ "run_id": run_id })),
    }]
}
