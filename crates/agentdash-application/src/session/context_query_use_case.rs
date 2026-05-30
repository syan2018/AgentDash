use uuid::Uuid;

use crate::error::ApplicationError;
use crate::session::construction::{ResolvedSessionOwner, SessionConstructionPlan};
use crate::session::construction_planner::SessionConstructionPlanner;
use crate::session::construction_provider::SessionConstructionProviderInput;
use crate::session::construction_use_case::{
    SessionConstructionProjectionMode, SessionConstructionUseCaseDeps,
    finalize_session_construction_projection,
};
use crate::session::{LaunchCommand, RuntimeCommandRecord, SessionMeta, UserPromptInput};
use agentdash_domain::project::Project;
use agentdash_domain::story::Story;
use agentdash_domain::task::AgentBinding;
use agentdash_spi::AuthIdentity;

pub enum SessionContextQueryOwnerFacts {
    Task {
        task_id: Uuid,
        workspace_id: Option<Uuid>,
        agent_binding: AgentBinding,
    },
    Story {
        story: Story,
    },
    Project {
        project: Project,
        binding_label: String,
    },
}

pub struct SessionContextQueryInput {
    pub session_id: String,
    pub owner: ResolvedSessionOwner,
    pub owner_facts: SessionContextQueryOwnerFacts,
    pub session_meta: SessionMeta,
    pub identity: Option<AuthIdentity>,
    pub had_existing_runtime: bool,
    pub requested_runtime_commands: Vec<RuntimeCommandRecord>,
}

pub async fn build_session_context_plan(
    deps: &SessionConstructionUseCaseDeps<'_>,
    input: SessionContextQueryInput,
) -> Result<Option<SessionConstructionPlan>, ApplicationError> {
    let mut plan = match input.owner_facts {
        SessionContextQueryOwnerFacts::Task {
            task_id,
            workspace_id,
            agent_binding,
        } => {
            SessionConstructionPlanner::plan_task_context_query(
                deps.repos,
                &deps.services.vfs_service,
                deps.services.extra_skill_dirs,
                &deps.config.platform_config,
                input.session_id.clone(),
                input.owner,
                task_id,
                workspace_id,
                agent_binding,
                Some(&input.session_meta),
            )
            .await
        }
        SessionContextQueryOwnerFacts::Story { story } => {
            let Some(plan) = SessionConstructionPlanner::plan_story_context_query(
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
        } => SessionConstructionPlanner::plan_project_context_query(
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
    };
    plan = finalize_session_construction_projection(
        deps,
        plan,
        Vec::new(),
        None,
        &facts,
        SessionConstructionProjectionMode::Inspect,
    )
    .await?;

    Ok(Some(plan))
}
