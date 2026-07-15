use agentdash_spi::{ExecutionContext, PlatformToolExecutionContext};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPlanScope {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunTaskScopeInput {
    pub owner: Option<PlatformToolExecutionContext>,
}

impl AgentRunTaskScopeInput {
    pub fn from_execution_context(context: &ExecutionContext) -> Self {
        Self {
            owner: context.turn.platform_tool_execution.clone(),
        }
    }
}

#[derive(Clone)]
pub struct AgentRunTaskScopeResolver;

impl AgentRunTaskScopeResolver {
    pub fn resolve(
        &self,
        input: &AgentRunTaskScopeInput,
    ) -> Result<TaskPlanScope, AgentRunTaskScopeResolutionError> {
        let owner = input
            .owner
            .as_ref()
            .ok_or(AgentRunTaskScopeResolutionError::MissingOwnerContext)?;
        Ok(TaskPlanScope {
            project_id: owner.project_id,
            run_id: owner.run_id,
            agent_id: Some(owner.agent_id),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentRunTaskScopeResolutionError {
    #[error("当前 Platform Tool 调用缺少 typed owner context，无法定位 Task scope")]
    MissingOwnerContext,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use agentdash_agent_runtime_contract::{RuntimeDriverGeneration, ToolSetRevision};

    fn runtime_id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid runtime id")
    }

    #[test]
    fn resolver_uses_typed_owner_context_without_repository_inference() {
        let resolver = AgentRunTaskScopeResolver;
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let scope = resolver
            .resolve(&AgentRunTaskScopeInput {
                owner: Some(PlatformToolExecutionContext {
                    run_id,
                    project_id,
                    agent_id,
                    frame_id: Uuid::new_v4(),
                    runtime_thread_id: runtime_id("session-1"),
                    presentation_thread_id: "presentation-1".parse().expect("presentation thread"),
                    visible_workspace_module_refs: Vec::new(),
                    invocation: Some(agentdash_spi::PlatformToolInvocationCoordinates {
                        runtime_turn_id: runtime_id("turn-1"),
                        runtime_item_id: runtime_id("item-1"),
                        presentation_item_id: runtime_id("turn-1:tool-1"),
                        source_thread_id: runtime_id("source-task-thread"),
                        source_turn_id: runtime_id("source-task-turn"),
                        source_item_id: runtime_id("source-task-item"),
                        binding_id: runtime_id("binding-task-scope"),
                        binding_generation: RuntimeDriverGeneration(1),
                        tool_set_revision: ToolSetRevision(1),
                    }),
                    launch_evidence_frame_id: Uuid::new_v4(),
                    current_surface_frame_id: Uuid::new_v4(),
                    orchestration_id: Some(Uuid::new_v4()),
                    node_path: Some("root/task".to_string()),
                    node_attempt: Some(2),
                }),
            })
            .expect("scope");

        assert_eq!(
            scope,
            TaskPlanScope {
                project_id,
                run_id,
                agent_id: Some(agent_id),
            }
        );
    }

    #[test]
    fn resolver_rejects_missing_typed_owner_context() {
        let resolver = AgentRunTaskScopeResolver;

        let error = resolver
            .resolve(&AgentRunTaskScopeInput { owner: None })
            .unwrap_err();

        assert!(matches!(
            error,
            AgentRunTaskScopeResolutionError::MissingOwnerContext
        ));
    }
}
