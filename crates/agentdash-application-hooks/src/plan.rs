use std::collections::BTreeSet;

use agentdash_agent_runtime_contract::{
    HookAction, HookDefinitionId, HookExecutionSite, HookFailurePolicy, HookPlanRevision,
    HookPoint, HookRequirement, SemanticStrength,
};
use agentdash_application_ports::agent_frame_hook_plan::{
    AgentFrameHookPlan, AgentFrameHookPlanCompileError, AgentFrameHookPlanCompileQuery,
    AgentFrameHookPlanCompiler, AgentFrameHookRequirement,
};
use agentdash_domain::workflow::WorkflowHookTrigger;
use agentdash_spi::{AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery, ExecutionHookProvider};
use async_trait::async_trait;

use crate::AppExecutionHookProvider;
use crate::snapshot_helpers::active_workflow_hook_rules;

const BEFORE_TOOL_DEFINITION_ID: &str = "agentdash.frame.before_tool";

#[async_trait]
impl AgentFrameHookPlanCompiler for AppExecutionHookProvider {
    async fn compile_agent_frame_hook_plan(
        &self,
        query: AgentFrameHookPlanCompileQuery,
    ) -> Result<AgentFrameHookPlan, AgentFrameHookPlanCompileError> {
        let snapshot = self
            .load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: query.target,
                provenance: query.provenance,
            })
            .await
            .map_err(|error| AgentFrameHookPlanCompileError::SourceUnavailable {
                message: error.to_string(),
            })?;
        let requirements = compile_requirements(&snapshot)?;
        AgentFrameHookPlan::compile(HookPlanRevision(1), requirements)
    }
}

fn compile_requirements(
    snapshot: &AgentFrameHookSnapshot,
) -> Result<Vec<AgentFrameHookRequirement>, AgentFrameHookPlanCompileError> {
    let mut before_tool_actions = BTreeSet::new();
    for rule in active_workflow_hook_rules(snapshot)
        .iter()
        .filter(|rule| rule.enabled)
    {
        match rule.trigger {
            WorkflowHookTrigger::BeforeTool
                if rule.preset.as_deref() == Some("supervised_tool_gate") =>
            {
                before_tool_actions.insert(HookAction::RequestApproval);
            }
            WorkflowHookTrigger::BeforeTool | WorkflowHookTrigger::AfterTool => {
                return Err(AgentFrameHookPlanCompileError::UnsupportedPolicy {
                    message: format!(
                        "hook rule `{}` at {:?} has no declared immutable action contract",
                        rule.key, rule.trigger
                    ),
                });
            }
            _ => {}
        }
    }

    if before_tool_actions.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![AgentFrameHookRequirement {
        definition_id: HookDefinitionId::new(BEFORE_TOOL_DEFINITION_ID).map_err(|error| {
            AgentFrameHookPlanCompileError::Digest {
                message: error.to_string(),
            }
        })?,
        requirement: HookRequirement {
            point: HookPoint::BeforeTool,
            actions: before_tool_actions,
            minimum_strength: SemanticStrength::ExactSynchronous,
            failure_policy: HookFailurePolicy::FailClosed,
            required: true,
        },
        site: HookExecutionSite::ToolBroker,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use agentdash_application_ports::hook_workflow_projection::{
        HookExecutionLogAppendCommand, HookWorkflowProjection, HookWorkflowProjectionError,
        HookWorkflowProjectionPort, HookWorkflowProjectionQuery,
    };
    use agentdash_domain::workflow::{WorkflowHookRuleSpec, WorkflowHookTrigger};
    use agentdash_spi::{HookControlTarget, RuntimeAdapterProvenance};
    use uuid::Uuid;

    struct EmptyProjection;

    #[async_trait]
    impl HookWorkflowProjectionPort for EmptyProjection {
        async fn load_hook_workflow_projection(
            &self,
            _query: HookWorkflowProjectionQuery,
        ) -> Result<HookWorkflowProjection, HookWorkflowProjectionError> {
            Ok(HookWorkflowProjection {
                run_context: None,
                active_workflow: None,
            })
        }

        async fn append_execution_log(
            &self,
            _command: HookExecutionLogAppendCommand,
        ) -> Result<(), HookWorkflowProjectionError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn default_owner_sources_compile_an_empty_hook_plan() {
        let provider = AppExecutionHookProvider::new(crate::AppExecutionHookProviderDeps {
            workflow_projection: Arc::new(EmptyProjection),
            script_evaluator: Arc::new(crate::test_script_evaluator::TestHookScriptEvaluator::new(
                &[],
            )),
        });
        let plan = provider
            .compile_agent_frame_hook_plan(AgentFrameHookPlanCompileQuery {
                target: HookControlTarget {
                    run_id: Uuid::new_v4(),
                    agent_id: Uuid::new_v4(),
                    frame_id: Uuid::new_v4(),
                },
                provenance: RuntimeAdapterProvenance::runtime_session(
                    "hook-plan-test",
                    None,
                    "hook-plan-test",
                ),
            })
            .await
            .expect("compile HookPlan");
        assert_eq!(plan.revision, HookPlanRevision(1));
        assert!(plan.requirements.is_empty());
    }

    #[test]
    fn permission_policy_without_workflow_rule_stays_empty() {
        let snapshot = crate::test_fixtures::snapshot_with_supervised_policy();
        assert!(compile_requirements(&snapshot).unwrap().is_empty());
    }

    #[test]
    fn explicit_supervised_gate_is_owned_by_tool_broker() {
        let mut snapshot =
            crate::test_fixtures::snapshot_with_workflow("implement", "session_ended");
        snapshot
            .metadata
            .as_mut()
            .and_then(|meta| meta.active_workflow.as_mut())
            .and_then(|workflow| workflow.effective_contract.as_mut())
            .expect("effective workflow contract")
            .hook_rules
            .push(WorkflowHookRuleSpec {
                key: "supervised-tools".to_string(),
                trigger: WorkflowHookTrigger::BeforeTool,
                description: "explicit supervised gate".to_string(),
                preset: Some("supervised_tool_gate".to_string()),
                params: None,
                script: None,
                enabled: true,
            });
        let requirements = compile_requirements(&snapshot).expect("compile requirements");
        assert_eq!(requirements.len(), 1);
        assert_eq!(
            requirements[0].requirement.actions,
            BTreeSet::from([HookAction::RequestApproval])
        );
        assert_eq!(requirements[0].site, HookExecutionSite::ToolBroker);
    }
}
