//! **DEPRECATED**: Phase 5A 已将所有 API 消费者迁移到 frame-based read model
//! (`AgentFrameSurfaceExt::typed_vfs()`)。此模块仅保留供 audit trace 路径参考，
//! 新代码不应引入对此模块的依赖。
#![allow(deprecated)]

use uuid::Uuid;

use crate::error::ApplicationError;
use crate::session::construction::{ResolvedSessionOwner, RuntimeContextInspectionPlan};
use crate::session::construction_planner::RuntimeContextInspectionPlanner;
use crate::session::construction_provider::SessionConstructionProviderInput;
use crate::session::construction_use_case::{
    SessionConstructionUseCaseDeps, finalize_session_construction_projection,
};
use crate::session::{LaunchCommand, RuntimeCommandRecord, SessionMeta, UserPromptInput};
use agentdash_domain::project::Project;
use agentdash_domain::story::Story;
use agentdash_domain::task::TaskDispatchPreference;
use agentdash_spi::AuthIdentity;

#[deprecated(note = "已被 frame-based read model 替代，参见 session_construction::resolve_session_frame_vfs")]
pub enum SessionContextQueryOwnerFacts {
    Task {
        task_id: Uuid,
        workspace_id: Option<Uuid>,
        dispatch_preference: TaskDispatchPreference,
    },
    Story {
        story: Story,
    },
    Project {
        project: Project,
        binding_label: String,
    },
}

#[deprecated(note = "已被 frame-based read model 替代")]
pub struct SessionContextQueryInput {
    pub session_id: String,
    pub owner: ResolvedSessionOwner,
    pub owner_facts: SessionContextQueryOwnerFacts,
    pub session_meta: SessionMeta,
    pub identity: Option<AuthIdentity>,
    pub had_existing_runtime: bool,
    pub requested_runtime_commands: Vec<RuntimeCommandRecord>,
}

#[deprecated(note = "已被 frame-based read model 替代，参见 session_construction::resolve_session_frame_vfs")]
pub async fn build_session_context_plan(
    deps: &SessionConstructionUseCaseDeps<'_>,
    input: SessionContextQueryInput,
) -> Result<Option<RuntimeContextInspectionPlan>, ApplicationError> {
    let mut plan = match input.owner_facts {
        SessionContextQueryOwnerFacts::Task {
            task_id,
            workspace_id,
            dispatch_preference,
        } => {
            RuntimeContextInspectionPlanner::plan_task_context_query(
                deps.repos,
                &deps.services.vfs_service,
                deps.services.extra_skill_dirs,
                &deps.config.platform_config,
                input.session_id.clone(),
                input.owner,
                task_id,
                workspace_id,
                dispatch_preference,
                Some(&input.session_meta),
            )
            .await
        }
        SessionContextQueryOwnerFacts::Story { story } => {
            let Some(plan) = RuntimeContextInspectionPlanner::plan_story_context_query(
                deps.repos,
                &deps.services.vfs_service,
                deps.services.extra_skill_dirs,
                &deps.config.platform_config,
                input.session_id.clone(),
                input.owner,
                &story,
                Some(&input.session_meta),
            )
            .await
            .map_err(ApplicationError::Internal)?
            else {
                return Ok(None);
            };
            plan
        }
        SessionContextQueryOwnerFacts::Project {
            project,
            binding_label,
        } => RuntimeContextInspectionPlanner::plan_project_context_query(
            deps.repos,
            &deps.services.vfs_service,
            deps.services.extra_skill_dirs,
            &deps.config.platform_config,
            input.session_id.clone(),
            input.owner,
            &project,
            &binding_label,
            &input.session_meta,
        )
        .await
        .map_err(|error| {
            if error.starts_with("无效的项目 Agent session label")
                || error.starts_with("Project Agent `")
            {
                ApplicationError::NotFound(error)
            } else {
                ApplicationError::Internal(error)
            }
        })?,
    };

    let user_input = UserPromptInput {
        prompt_blocks: None,
        env: Default::default(),
        executor_config: input.session_meta.executor_config.clone(),
        backend_selection: None,
    };
    let facts = SessionConstructionProviderInput {
        session_id: input.session_id.clone(),
        command: LaunchCommand::http_prompt_input(user_input, input.identity),
        session_meta: input.session_meta,
        had_existing_runtime: input.had_existing_runtime,
        requested_runtime_commands: input.requested_runtime_commands,
        agent_needs_bootstrap: false,
    };
    plan = finalize_session_construction_projection(deps, plan, Vec::new(), None, &facts).await?;

    Ok(Some(plan))
}
