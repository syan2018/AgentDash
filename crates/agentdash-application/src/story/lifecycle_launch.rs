use uuid::Uuid;

use agentdash_domain::agent::ProjectAgent;
use agentdash_domain::story::Story;
use agentdash_domain::workflow::{
    AgentLaunchIntent, AgentPolicy, AgentRuntimeRefs, CapabilityPolicy, ContextPolicy,
    ExecutionSource, RuntimePolicy, SubjectRef, WorkflowGraphRef,
};

use crate::ApplicationError;
use crate::repository_set::RepositorySet;
use crate::workflow::{LifecycleDispatchService, WorkflowApplicationError};

#[derive(Debug, Clone)]
pub struct StoryLifecycleLaunchCommand {
    pub story_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct StoryLifecycleLaunchResult {
    pub story_id: Uuid,
    pub project_agent_id: Uuid,
    pub runtime_refs: AgentRuntimeRefs,
    pub delivery_runtime_ref: Option<Uuid>,
    pub subject_ref: SubjectRef,
}

pub struct StoryLifecycleLaunchService {
    pub repos: RepositorySet,
}

impl StoryLifecycleLaunchService {
    pub async fn launch_story(
        &self,
        command: StoryLifecycleLaunchCommand,
    ) -> Result<StoryLifecycleLaunchResult, ApplicationError> {
        let story = self
            .repos
            .story_repo
            .get_by_id(command.story_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::NotFound(format!("Story {} 不存在", command.story_id))
            })?;
        let project_agent = resolve_story_root_project_agent(&self.repos, story.project_id).await?;
        let intent = build_story_root_launch_intent(&story, &project_agent);

        let dispatch_service = LifecycleDispatchService::new(
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.workflow_graph_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.agent_frame_repo.as_ref(),
            self.repos.lifecycle_subject_association_repo.as_ref(),
            self.repos.lifecycle_gate_repo.as_ref(),
            self.repos.agent_lineage_repo.as_ref(),
        )
        .with_anchor_repo(self.repos.execution_anchor_repo.as_ref())
        .with_runtime_session_creator(self.repos.runtime_session_creator.as_ref());

        let dispatch_result = dispatch_service
            .launch_agent(&intent)
            .await
            .map_err(map_workflow_error)?;

        if let Some(mut lifecycle_agent) = self
            .repos
            .lifecycle_agent_repo
            .get(dispatch_result.runtime_refs.agent_ref)
            .await?
        {
            lifecycle_agent.project_agent_id = Some(project_agent.id);
            self.repos
                .lifecycle_agent_repo
                .update(&lifecycle_agent)
                .await?;
        }

        Ok(StoryLifecycleLaunchResult {
            story_id: story.id,
            project_agent_id: project_agent.id,
            runtime_refs: dispatch_result.runtime_refs,
            delivery_runtime_ref: dispatch_result.delivery_runtime_ref,
            subject_ref: intent
                .subject_ref
                .expect("story launch always carries subject_ref"),
        })
    }
}

pub fn build_story_root_launch_intent(
    story: &Story,
    project_agent: &ProjectAgent,
) -> AgentLaunchIntent {
    let workflow_graph_ref = project_agent
        .default_lifecycle_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(|key| WorkflowGraphRef::ByKey {
            project_id: story.project_id,
            key: key.to_string(),
        });

    AgentLaunchIntent {
        project_id: story.project_id,
        source: ExecutionSource::User,
        subject_ref: Some(SubjectRef::new("story", story.id)),
        parent_run_id: None,
        parent_agent_id: None,
        workflow_graph_ref,
        agent_procedure_ref: None,
        run_policy: agentdash_domain::workflow::RunPolicy::CreateLinkedRun,
        agent_policy: AgentPolicy::Create,
        context_policy: ContextPolicy::Isolated,
        capability_policy: CapabilityPolicy::Baseline,
        runtime_policy: RuntimePolicy::CreateRuntimeSession,
    }
}

pub async fn resolve_story_root_project_agent(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<ProjectAgent, ApplicationError> {
    repos
        .project_agent_repo
        .list_by_project(project_id)
        .await?
        .into_iter()
        .find(|agent| agent.is_default_for_story)
        .ok_or_else(|| {
            ApplicationError::InvalidConfig(format!("Project {project_id} 缺少默认 Story Agent"))
        })
}

fn map_workflow_error(error: WorkflowApplicationError) -> ApplicationError {
    match error {
        WorkflowApplicationError::BadRequest(message) => ApplicationError::BadRequest(message),
        WorkflowApplicationError::NotFound(message) => ApplicationError::NotFound(message),
        WorkflowApplicationError::Conflict(message) => ApplicationError::Conflict(message),
        WorkflowApplicationError::Internal(message) => ApplicationError::Internal(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn story_root_launch_intent_uses_story_subject_and_default_graph() {
        let mut story = Story::new(Uuid::new_v4(), "story".to_string(), String::new());
        let mut project_agent = ProjectAgent::new(
            story.project_id,
            "story-owner".to_string(),
            "PI_AGENT".to_string(),
        );
        project_agent.is_default_for_story = true;
        project_agent.default_lifecycle_key = Some("story.lifecycle".to_string());

        let intent = build_story_root_launch_intent(&story, &project_agent);

        assert_eq!(intent.subject_ref, Some(SubjectRef::new("story", story.id)));
        assert_eq!(intent.project_id, story.project_id);
        assert_eq!(intent.source, ExecutionSource::User);
        assert_eq!(
            intent.run_policy,
            agentdash_domain::workflow::RunPolicy::CreateLinkedRun
        );
        assert_eq!(intent.agent_policy, AgentPolicy::Create);
        assert_eq!(intent.context_policy, ContextPolicy::Isolated);
        assert_eq!(intent.runtime_policy, RuntimePolicy::CreateRuntimeSession);
        assert!(matches!(
            intent.workflow_graph_ref,
            Some(WorkflowGraphRef::ByKey { key, .. }) if key == "story.lifecycle"
        ));

        story.id = Uuid::new_v4();
        project_agent.default_lifecycle_key = None;
        let graphless_intent = build_story_root_launch_intent(&story, &project_agent);
        assert!(graphless_intent.workflow_graph_ref.is_none());
    }
}
