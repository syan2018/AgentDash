use std::collections::{BTreeSet, HashSet};

use agentdash_application_ports::agent_frame_hook_plan::{
    AgentFrameHookPlan, AgentFrameHookPlanCompileError, AgentFrameHookPlanCompileQuery,
    AgentFrameHookPlanCompiler, AgentFrameHookRequirement, HookAction, HookDefinitionId,
    HookExecutionSite, HookFailurePolicy, HookPlanRevision, HookPoint, HookRequirement,
    SemanticStrength,
};
use agentdash_domain::workflow::{WorkflowHookRuleSpec, WorkflowHookTrigger};
use agentdash_platform_spi::{AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery};
use async_trait::async_trait;

use crate::AppExecutionHookProvider;
use crate::rules::product_hook_rules;

#[async_trait]
impl AgentFrameHookPlanCompiler for AppExecutionHookProvider {
    async fn compile_agent_frame_hook_plan(
        &self,
        query: AgentFrameHookPlanCompileQuery,
    ) -> Result<AgentFrameHookPlan, AgentFrameHookPlanCompileError> {
        let snapshot = self
            .load_product_hook_snapshot(AgentFrameHookSnapshotQuery {
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
    let mut seen = HashSet::new();
    product_hook_rules(snapshot)
        .into_iter()
        .map(|rule| {
            if !seen.insert(rule.key.clone()) {
                return Err(AgentFrameHookPlanCompileError::UnsupportedPolicy {
                    message: format!("duplicate Product hook rule key `{}`", rule.key),
                });
            }
            requirement_from_rule(&rule)
        })
        .collect()
}

fn requirement_from_rule(
    rule: &WorkflowHookRuleSpec,
) -> Result<AgentFrameHookRequirement, AgentFrameHookPlanCompileError> {
    let actions = actions_for_trigger(rule.trigger);
    let failure_policy = if actions.iter().any(|action| {
        matches!(
            action,
            HookAction::Block
                | HookAction::RewriteInput
                | HookAction::RewriteResult
                | HookAction::RequestApproval
        )
    }) {
        HookFailurePolicy::FailClosed
    } else {
        HookFailurePolicy::FailOpenWithDiagnostic
    };
    let site = site_for_trigger(rule.trigger);
    Ok(AgentFrameHookRequirement {
        definition_id: HookDefinitionId::new(format!("workflow-hook:{}", rule.key)).map_err(
            |error| AgentFrameHookPlanCompileError::UnsupportedPolicy {
                message: error.to_string(),
            },
        )?,
        requirement: HookRequirement {
            point: point_for_trigger(rule.trigger),
            actions,
            minimum_strength: match site {
                HookExecutionSite::ObservedEventReaction => SemanticStrength::ObservedOnly,
                HookExecutionSite::ManagedRuntime | HookExecutionSite::ToolBroker => {
                    SemanticStrength::ExactDurableBoundary
                }
                HookExecutionSite::AgentCoreCallback | HookExecutionSite::DriverNative => {
                    SemanticStrength::ExactSynchronous
                }
            },
            failure_policy,
            required: true,
        },
        site,
    })
}

fn point_for_trigger(trigger: WorkflowHookTrigger) -> HookPoint {
    match trigger {
        WorkflowHookTrigger::UserPromptSubmit => HookPoint::BeforeTurn,
        WorkflowHookTrigger::BeforeTool | WorkflowHookTrigger::BeforeSubagentDispatch => {
            HookPoint::BeforeTool
        }
        WorkflowHookTrigger::AfterTool
        | WorkflowHookTrigger::AfterSubagentDispatch
        | WorkflowHookTrigger::CompanionResult => HookPoint::AfterTool,
        WorkflowHookTrigger::AfterTurn => HookPoint::AfterTurn,
        WorkflowHookTrigger::BeforeStop => HookPoint::BeforeStop,
        WorkflowHookTrigger::SessionTerminal => HookPoint::AfterItem,
        WorkflowHookTrigger::BeforeCompact => HookPoint::BeforeContextCompact,
        WorkflowHookTrigger::AfterCompact => HookPoint::AfterContextCompact,
        WorkflowHookTrigger::BeforeProviderRequest => HookPoint::BeforeProviderRequest,
    }
}

fn site_for_trigger(trigger: WorkflowHookTrigger) -> HookExecutionSite {
    match trigger {
        WorkflowHookTrigger::UserPromptSubmit
        | WorkflowHookTrigger::BeforeCompact
        | WorkflowHookTrigger::AfterCompact => HookExecutionSite::ManagedRuntime,
        WorkflowHookTrigger::BeforeSubagentDispatch
        | WorkflowHookTrigger::AfterSubagentDispatch
        | WorkflowHookTrigger::CompanionResult => HookExecutionSite::ToolBroker,
        WorkflowHookTrigger::SessionTerminal => HookExecutionSite::ObservedEventReaction,
        WorkflowHookTrigger::BeforeTool
        | WorkflowHookTrigger::AfterTool
        | WorkflowHookTrigger::AfterTurn
        | WorkflowHookTrigger::BeforeStop
        | WorkflowHookTrigger::BeforeProviderRequest => HookExecutionSite::AgentCoreCallback,
    }
}

fn actions_for_trigger(trigger: WorkflowHookTrigger) -> BTreeSet<HookAction> {
    use HookAction::*;
    match trigger {
        WorkflowHookTrigger::UserPromptSubmit => {
            BTreeSet::from([Observe, AddContext, Block, RewriteInput])
        }
        WorkflowHookTrigger::BeforeTool => BTreeSet::from([
            Observe,
            AddContext,
            Block,
            RewriteInput,
            RequestApproval,
            RefreshSurface,
            EmitEffect,
        ]),
        WorkflowHookTrigger::AfterTool => BTreeSet::from([
            Observe,
            AddContext,
            RewriteResult,
            RefreshSurface,
            EmitEffect,
        ]),
        WorkflowHookTrigger::AfterTurn => BTreeSet::from([
            Observe,
            AddContext,
            ContinueTurn,
            RefreshSurface,
            EmitEffect,
        ]),
        WorkflowHookTrigger::BeforeStop => {
            BTreeSet::from([Observe, Block, ContinueTurn, RefreshSurface, EmitEffect])
        }
        WorkflowHookTrigger::SessionTerminal => BTreeSet::from([Observe, EmitEffect]),
        WorkflowHookTrigger::BeforeSubagentDispatch => {
            BTreeSet::from([Observe, AddContext, Block, RewriteInput, EmitEffect])
        }
        WorkflowHookTrigger::AfterSubagentDispatch | WorkflowHookTrigger::CompanionResult => {
            BTreeSet::from([Observe, AddContext, ContinueTurn, EmitEffect])
        }
        WorkflowHookTrigger::BeforeCompact => {
            BTreeSet::from([Observe, AddContext, Block, EmitEffect])
        }
        WorkflowHookTrigger::AfterCompact => {
            BTreeSet::from([Observe, AddContext, RefreshSurface, EmitEffect])
        }
        WorkflowHookTrigger::BeforeProviderRequest => {
            BTreeSet::from([Observe, AddContext, Block, RewriteInput])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use agentdash_application_ports::hook_workflow_projection::{
        HookExecutionLogAppendCommand, HookWorkflowProjection, HookWorkflowProjectionError,
        HookWorkflowProjectionPort, HookWorkflowProjectionQuery,
    };
    use agentdash_platform_spi::{HookControlTarget, RuntimeAdapterProvenance};
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
                provenance: RuntimeAdapterProvenance::runtime_thread(
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
    fn workflow_policy_compiles_to_required_product_semantics() {
        let snapshot = crate::test_fixtures::snapshot_with_workflow("check", "checklist_passed");

        let requirements = compile_requirements(&snapshot).expect("compile Product hook policy");

        assert_eq!(requirements.len(), 1);
        let requirement = &requirements[0];
        assert_eq!(requirement.requirement.point, HookPoint::BeforeStop);
        assert_eq!(requirement.site, HookExecutionSite::AgentCoreCallback);
        assert!(requirement.requirement.required);
        assert!(requirement.requirement.actions.contains(&HookAction::Block));
    }
}
