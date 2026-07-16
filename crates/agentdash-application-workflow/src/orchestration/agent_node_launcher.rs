use std::sync::Arc;

use agentdash_agent_runtime_contract::PresentationThreadId;
use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeProvisionRequest, AgentRunRuntimeProvisioner, AgentRunRuntimeTarget,
};
use agentdash_application_ports::lifecycle_materialization::{
    WorkflowAgentNodeMaterializationPort, WorkflowAgentNodeMaterializationRequest,
};
use agentdash_application_ports::workflow_agent_run_delivery::{
    WorkflowAgentRunDeliveryCommand, WorkflowAgentRunDeliveryPort,
};
use agentdash_domain::workflow::{
    AgentProcedureExecutionSpec, AgentProcedureRepository, AgentReusePolicy, ExecutorSpec,
    LifecycleRun, OrchestrationBindingRefs, RuntimePolicy, RuntimeSessionPolicy,
};

use crate::WorkflowApplicationError;

use super::executor_launcher::LaunchedAgentNode;
use super::ready_node::{ReadyNodeView, RuntimeNodeCoordinate};
use super::runtime::OrchestrationRuntimeEvent;

#[derive(Clone)]
pub(super) struct AgentNodeLauncher {
    agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    workflow_agent_node_materialization: Arc<dyn WorkflowAgentNodeMaterializationPort>,
    runtime_provisioner: Arc<dyn AgentRunRuntimeProvisioner>,
    workflow_delivery: Arc<dyn WorkflowAgentRunDeliveryPort>,
}

impl AgentNodeLauncher {
    pub(super) fn new(
        agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
        workflow_agent_node_materialization: Arc<dyn WorkflowAgentNodeMaterializationPort>,
        runtime_provisioner: Arc<dyn AgentRunRuntimeProvisioner>,
        workflow_delivery: Arc<dyn WorkflowAgentRunDeliveryPort>,
    ) -> Self {
        Self {
            agent_procedure_repo,
            workflow_agent_node_materialization,
            runtime_provisioner,
            workflow_delivery,
        }
    }

    pub(super) async fn launch(
        &self,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
    ) -> Result<AgentNodeLaunchOutcome, WorkflowApplicationError> {
        let ready_node = ReadyNodeView::for_coordinate(run, coordinate)?;
        let plan_node = ready_node.plan_node;
        let executor = plan_node.executor.clone();
        let Some(ExecutorSpec::AgentProcedure {
            procedure,
            agent_reuse_policy,
            runtime_session_policy,
        }) = executor
        else {
            return Ok(AgentNodeLaunchOutcome::blocked(
                "agent_executor_missing",
                "AgentCall node 缺少 AgentProcedure executor spec",
                false,
            ));
        };

        let loaded_workflow =
            if let AgentProcedureExecutionSpec::ByKey { procedure_key } = &procedure {
                match self
                    .agent_procedure_repo
                    .get_by_project_and_key(run.project_id, procedure_key)
                    .await?
                {
                    Some(workflow) => Some(workflow),
                    None => {
                        return Ok(AgentNodeLaunchOutcome::blocked(
                            "agent_procedure_not_found",
                            format!("AgentProcedure `{procedure_key}` 不存在"),
                            false,
                        ));
                    }
                }
            } else {
                None
            };
        let workflow_contract = procedure.snapshot_contract().cloned().or_else(|| {
            loaded_workflow
                .as_ref()
                .map(|workflow| workflow.contract.clone())
        });

        let runtime_policy = match (agent_reuse_policy, runtime_session_policy) {
            (AgentReusePolicy::CreateActivityAgent, RuntimeSessionPolicy::CreateNew) => {
                RuntimePolicy::ProvisionRuntimeThread
            }
            (
                AgentReusePolicy::ContinueCurrentAgent,
                RuntimeSessionPolicy::DeliverToCurrentTrace,
            ) => {
                return Ok(AgentNodeLaunchOutcome::blocked(
                    "agent_executor_policy_not_supported",
                    "ContinueCurrentAgent + DeliverToCurrentTrace 需要 connector delivery surface，当前 orchestration executor 不伪造已投递状态",
                    false,
                ));
            }
            _ => {
                return Ok(AgentNodeLaunchOutcome::blocked(
                    "agent_executor_policy_not_supported",
                    "AgentCall executor policy 当前 scheduler 不支持",
                    false,
                ));
            }
        };
        let Some(delivery_message) =
            workflow_delivery_message(workflow_contract.as_ref(), ready_node.runtime_node)
        else {
            return Ok(AgentNodeLaunchOutcome::blocked(
                "workflow_agent_delivery_input_missing",
                "AgentCall node 没有可直接投递的 typed guidance 或单一字符串 input",
                false,
            ));
        };

        let orchestration_binding = OrchestrationBindingRefs::new(
            coordinate.orchestration_id,
            coordinate.node_path.clone(),
            coordinate.attempt,
        );
        let materialized = self
            .workflow_agent_node_materialization
            .materialize_workflow_agent_node(WorkflowAgentNodeMaterializationRequest {
                run_id: run.id,
                orchestration_binding,
                runtime_policy,
                frame_created_by_id: Some(format!(
                    "{}:{}#{}",
                    coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
                )),
                workflow_contract: workflow_contract.clone(),
            })
            .await?;
        let binding = self
            .runtime_provisioner
            .provision(&AgentRunRuntimeProvisionRequest {
                target: AgentRunRuntimeTarget {
                    run_id: materialized.runtime_refs.run_ref,
                    agent_id: materialized.runtime_refs.agent_ref,
                },
                presentation_thread_id: PresentationThreadId::new(
                    materialized.delivery_runtime_ref.to_string(),
                )
                .expect("delivery runtime ref is a non-empty presentation thread id"),
                identity: None,
                backend_selection: None,
                fork: None,
                terminal_hook_effect_binding: None,
            })
            .await
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!(
                    "AgentRun Runtime provision 失败: {error}"
                ))
            })?;
        let session_id = binding.presentation_thread_id.to_string();
        self.workflow_delivery
            .deliver(WorkflowAgentRunDeliveryCommand {
                target: AgentRunRuntimeTarget {
                    run_id: materialized.runtime_refs.run_ref,
                    agent_id: materialized.runtime_refs.agent_ref,
                },
                presentation_thread_id: binding.presentation_thread_id.clone(),
                client_command_id: format!(
                    "workflow:{}:{}#{}",
                    coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
                ),
                input: vec![agentdash_agent_runtime_contract::RuntimeInput::text(
                    delivery_message.clone(),
                )],
                presentation_content: agentdash_agent_protocol::text_user_input_blocks(
                    delivery_message,
                ),
                actor: agentdash_agent_runtime_contract::RuntimeActor::System {
                    component: "workflow_orchestrator".into(),
                },
                orchestration_id: coordinate.orchestration_id,
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
            })
            .await
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!(
                    "workflow AgentRun mailbox delivery 失败: {error}"
                ))
            })?;

        Ok(AgentNodeLaunchOutcome::Launched {
            launched: LaunchedAgentNode {
                run_id: materialized.runtime_refs.run_ref,
                orchestration_id: coordinate.orchestration_id,
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                runtime_session_id: session_id.clone(),
            },
            event: Box::new(OrchestrationRuntimeEvent::NodeClaimed {
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                timestamp: chrono::Utc::now(),
            }),
        })
    }
}

fn workflow_delivery_message(
    contract: Option<&agentdash_domain::workflow::AgentProcedureContract>,
    runtime_node: &agentdash_domain::workflow::RuntimeNodeState,
) -> Option<String> {
    if let Some(guidance) = contract
        .and_then(|contract| contract.injection.guidance.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(guidance.to_string());
    }
    let declared_inputs = contract?
        .input_ports
        .iter()
        .map(|port| port.key.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut inputs = runtime_node
        .inputs
        .iter()
        .filter(|input| declared_inputs.contains(input.port_key.as_str()))
        .filter_map(|input| input.value.as_str().map(str::trim))
        .filter(|value| !value.is_empty());
    let message = inputs.next()?.to_string();
    if inputs.next().is_some() {
        return None;
    }
    Some(message)
}

pub(super) enum AgentNodeLaunchOutcome {
    Launched {
        launched: LaunchedAgentNode,
        event: Box<OrchestrationRuntimeEvent>,
    },
    Blocked {
        code: String,
        message: String,
        retryable: bool,
    },
}

impl AgentNodeLaunchOutcome {
    fn blocked(code: &str, message: impl Into<String>, retryable: bool) -> Self {
        Self::Blocked {
            code: code.to_string(),
            message: message.into(),
            retryable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::{
        AgentProcedureContract, ContextStrategy, InputPortDefinition, StandaloneFulfillment,
    };

    fn contract(keys: &[&str]) -> AgentProcedureContract {
        AgentProcedureContract {
            input_ports: keys
                .iter()
                .map(|key| InputPortDefinition {
                    key: (*key).into(),
                    description: String::new(),
                    context_strategy: ContextStrategy::Full,
                    context_template: None,
                    standalone_fulfillment: StandaloneFulfillment::Required,
                })
                .collect(),
            ..AgentProcedureContract::default()
        }
    }

    fn runtime_node(inputs: serde_json::Value) -> agentdash_domain::workflow::RuntimeNodeState {
        serde_json::from_value(serde_json::json!({
            "node_id": "node-1",
            "node_path": "node-1",
            "kind": "agent_call",
            "inputs": inputs,
            "outputs": [],
            "children": [],
            "phase_path": [],
            "trace_refs": []
        }))
        .unwrap()
    }

    #[test]
    fn workflow_delivery_requires_one_declared_non_empty_string_without_guidance() {
        let contract = contract(&["prompt", "context"]);
        let single = runtime_node(serde_json::json!([
            {"port_key":"prompt", "value":"do the work"},
            {"port_key":"ignored", "value":"not declared"}
        ]));
        assert_eq!(
            workflow_delivery_message(Some(&contract), &single).as_deref(),
            Some("do the work")
        );

        let multiple = runtime_node(serde_json::json!([
            {"port_key":"prompt", "value":"first"},
            {"port_key":"context", "value":"second"}
        ]));
        assert!(workflow_delivery_message(Some(&contract), &multiple).is_none());

        let structured = runtime_node(serde_json::json!([
            {"port_key":"prompt", "value":{"typed":true}}
        ]));
        assert!(workflow_delivery_message(Some(&contract), &structured).is_none());
    }

    #[test]
    fn workflow_guidance_is_the_explicit_presentation_owner() {
        let mut contract = contract(&["prompt"]);
        contract.injection.guidance = Some(" owner guidance ".into());
        let structured = runtime_node(serde_json::json!([
            {"port_key":"prompt", "value":{"typed":true}}
        ]));
        assert_eq!(
            workflow_delivery_message(Some(&contract), &structured).as_deref(),
            Some("owner guidance")
        );
    }
}
