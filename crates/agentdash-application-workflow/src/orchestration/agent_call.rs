use std::{collections::BTreeSet, sync::Arc};

use agentdash_domain::{
    agent_run_target::AgentRunTarget,
    workflow::{
        ActivationRule, AgentProcedureContract, AgentProcedureExecutionSpec,
        AgentProcedureRepository, AgentReusePolicy, ExecutorRunRef, ExecutorSpec, LifecycleRun,
        RuntimeNodeState, RuntimeThreadPolicy, WorkflowAgentCallSourceBindingRef,
    },
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::WorkflowApplicationError;

use super::{
    ready_node::{ReadyNodeView, RuntimeNodeCoordinate},
    runtime::OrchestrationRuntimeEvent,
};

pub const WORKFLOW_AGENT_CALL_INPUT_PORT_SCHEMA_V1: &str =
    "agentdash.workflow.agent-call.input-port.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowAgentCallIdentity {
    pub request_id: String,
    pub lifecycle_run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowAgentCallTargetIntent {
    CreateNew {
        target: AgentRunTarget,
    },
    ContinueCurrent {
        target: AgentRunTarget,
        runtime_thread_id: String,
        source_binding: WorkflowAgentCallSourceBindingRef,
    },
}

impl WorkflowAgentCallTargetIntent {
    pub fn target(&self) -> &AgentRunTarget {
        match self {
            Self::CreateNew { target } | Self::ContinueCurrent { target, .. } => target,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowAgentCallContentBlock {
    Text { text: String },
    Structured { schema: String, value: Value },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowAgentCallRequest {
    pub identity: WorkflowAgentCallIdentity,
    pub payload_digest: String,
    pub project_id: Uuid,
    pub created_by_user_id: String,
    pub target_intent: WorkflowAgentCallTargetIntent,
    pub procedure_key: Option<String>,
    pub procedure_contract: AgentProcedureContract,
    pub input: Vec<WorkflowAgentCallContentBlock>,
}

impl WorkflowAgentCallRequest {
    fn calculated_payload_digest(&self) -> String {
        let canonical = serde_json::to_vec(&(
            &self.identity,
            self.project_id,
            &self.created_by_user_id,
            &self.target_intent,
            &self.procedure_key,
            &self.procedure_contract,
            &self.input,
        ))
        .expect("Workflow AgentCall request is serializable");
        format!("sha256:{:x}", Sha256::digest(canonical))
    }

    pub fn validate_payload_digest(&self) -> bool {
        self.payload_digest == self.calculated_payload_digest()
    }

    pub fn with_calculated_payload_digest(mut self) -> Self {
        self.payload_digest = self.calculated_payload_digest();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAgentCallMailboxState {
    Queued,
    Submitted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowAgentCallDispatchOutcome {
    Pending,
    Accepted {
        target: AgentRunTarget,
        runtime_thread_id: String,
        source_binding: WorkflowAgentCallSourceBindingRef,
        mailbox_state: WorkflowAgentCallMailboxState,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct WorkflowAgentCallDispatchError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl WorkflowAgentCallDispatchError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }
}

#[async_trait]
pub trait WorkflowAgentCallDispatchPort: Send + Sync {
    async fn dispatch(
        &self,
        request: WorkflowAgentCallRequest,
    ) -> Result<WorkflowAgentCallDispatchOutcome, WorkflowAgentCallDispatchError>;
}

#[derive(Clone)]
pub(super) struct WorkflowAgentCallLauncher {
    procedure_repo: Arc<dyn AgentProcedureRepository>,
    dispatch: Option<Arc<dyn WorkflowAgentCallDispatchPort>>,
}

impl WorkflowAgentCallLauncher {
    pub(super) fn new(procedure_repo: Arc<dyn AgentProcedureRepository>) -> Self {
        Self {
            procedure_repo,
            dispatch: None,
        }
    }

    pub(super) fn with_dispatch(
        mut self,
        dispatch: Arc<dyn WorkflowAgentCallDispatchPort>,
    ) -> Self {
        self.dispatch = Some(dispatch);
        self
    }

    pub(super) async fn launch(
        &self,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
    ) -> Result<WorkflowAgentCallLaunchOutcome, WorkflowApplicationError> {
        let ready = ReadyNodeView::for_coordinate(run, coordinate)?;
        let Some(ExecutorSpec::AgentProcedure {
            procedure,
            agent_reuse_policy,
            runtime_thread_policy,
        }) = ready.plan_node.executor.as_ref()
        else {
            return Ok(WorkflowAgentCallLaunchOutcome::blocked(
                "agent_executor_missing",
                "AgentCall node 缺少 AgentProcedure executor spec",
                false,
            ));
        };

        let target_intent = match (agent_reuse_policy, runtime_thread_policy) {
            (AgentReusePolicy::CreateActivityAgent, RuntimeThreadPolicy::CreateNew) => {
                WorkflowAgentCallTargetIntent::CreateNew {
                    target: ready
                        .runtime_node
                        .agent_call
                        .as_ref()
                        .map(|history| history.target.clone())
                        .unwrap_or_else(|| AgentRunTarget {
                            run_id: run.id,
                            agent_id: Uuid::new_v4(),
                        }),
                }
            }
            (
                AgentReusePolicy::ContinueCurrentAgent,
                RuntimeThreadPolicy::DeliverToCurrentThread,
            ) => match current_authority(run, coordinate, ready.runtime_node) {
                Ok(authority) => WorkflowAgentCallTargetIntent::ContinueCurrent {
                    target: authority.target,
                    runtime_thread_id: authority.runtime_thread_id,
                    source_binding: authority.source_binding,
                },
                Err(CurrentAuthorityError::Missing) => {
                    return Ok(WorkflowAgentCallLaunchOutcome::blocked(
                        "current_agent_run_authority_missing",
                        "ContinueCurrent AgentCall 缺少已派发 predecessor 的完整 Runtime authority",
                        false,
                    ));
                }
                Err(CurrentAuthorityError::Ambiguous) => {
                    return Ok(WorkflowAgentCallLaunchOutcome::blocked(
                        "current_agent_run_authority_ambiguous",
                        "ContinueCurrent AgentCall 找到多个已派发 predecessor Runtime authority",
                        false,
                    ));
                }
            },
            _ => {
                return Ok(WorkflowAgentCallLaunchOutcome::blocked(
                    "agent_executor_policy_not_supported",
                    "AgentCall executor policy 组合不受 Product protocol 支持",
                    false,
                ));
            }
        };

        let identity = WorkflowAgentCallIdentity {
            request_id: format!(
                "workflow-agent-call:{}:{}#{}",
                coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
            ),
            lifecycle_run_id: run.id,
            orchestration_id: coordinate.orchestration_id,
            node_path: coordinate.node_path.clone(),
            attempt: coordinate.attempt,
        };
        let request = if let Some(history) = ready.runtime_node.agent_call.as_ref() {
            let request: WorkflowAgentCallRequest = serde_json::from_value(history.request.clone())
                .map_err(|error| {
                    WorkflowApplicationError::Internal(format!(
                        "durable Workflow AgentCall request 无效: {error}"
                    ))
                })?;
            if request.identity != identity
                || !request.validate_payload_digest()
                || request.payload_digest != history.payload_digest
                || request.target_intent.target() != &history.target
                || request.target_intent != target_intent
            {
                return Ok(WorkflowAgentCallLaunchOutcome::blocked(
                    "agent_call_payload_conflict",
                    "AgentCall durable prepared request 与 history identity 不一致",
                    false,
                ));
            }
            request
        } else {
            let (procedure_key, contract) = self.resolve_procedure(run, procedure).await?;
            WorkflowAgentCallRequest {
                identity,
                payload_digest: String::new(),
                project_id: run.project_id,
                created_by_user_id: run.created_by_user_id.clone(),
                target_intent,
                procedure_key,
                input: compile_input(&contract, ready.runtime_node),
                procedure_contract: contract,
            }
            .with_calculated_payload_digest()
        };

        let target = request.target_intent.target().clone();
        let Some(history) = ready.runtime_node.agent_call.as_ref() else {
            return Ok(WorkflowAgentCallLaunchOutcome::Prepared {
                event: OrchestrationRuntimeEvent::AgentCallPrepared {
                    node_path: coordinate.node_path.clone(),
                    attempt: coordinate.attempt,
                    request_id: request.identity.request_id.clone(),
                    payload_digest: request.payload_digest.clone(),
                    target,
                    request: serde_json::to_value(&request).expect("AgentCall request serializes"),
                    runtime_thread_id: request.target_intent.runtime_thread_id().map(str::to_owned),
                    source_binding: request.target_intent.source_binding().cloned(),
                    timestamp: chrono::Utc::now(),
                },
            });
        };
        if history.request_id != request.identity.request_id
            || history.payload_digest != request.payload_digest
            || history.target != target
        {
            return Ok(WorkflowAgentCallLaunchOutcome::blocked(
                "agent_call_payload_conflict",
                "AgentCall retry payload 与 durable prepared history 不一致",
                false,
            ));
        }
        if let (Some(runtime_thread_id), Some(_)) =
            (&history.runtime_thread_id, history.dispatched_at)
        {
            let Some(source_binding) = history.source_binding.clone() else {
                return Ok(WorkflowAgentCallLaunchOutcome::blocked(
                    "agent_call_runtime_authority_incomplete",
                    "durable AgentCall history 缺少 Runtime source binding",
                    false,
                ));
            };
            return Ok(WorkflowAgentCallLaunchOutcome::Accepted {
                target: target.clone(),
                runtime_thread_id: runtime_thread_id.clone(),
                dispatch_event: OrchestrationRuntimeEvent::AgentCallStarted {
                    node_path: coordinate.node_path.clone(),
                    attempt: coordinate.attempt,
                    request_id: request.identity.request_id,
                    payload_digest: request.payload_digest,
                    target,
                    runtime_thread_id: runtime_thread_id.clone(),
                    source_binding,
                    claim_id: history.claim_id.clone().unwrap_or_else(|| {
                        format!(
                            "workflow-agent-call-claim:{}:{}#{}",
                            coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
                        )
                    }),
                    timestamp: chrono::Utc::now(),
                },
            });
        }

        let Some(dispatch) = &self.dispatch else {
            return Ok(WorkflowAgentCallLaunchOutcome::blocked(
                "agent_call_dispatch_not_composed",
                "Workflow AgentCall Product dispatch port 未注入",
                true,
            ));
        };
        match dispatch.dispatch(request.clone()).await {
            Ok(WorkflowAgentCallDispatchOutcome::Pending) => {
                Ok(WorkflowAgentCallLaunchOutcome::Pending)
            }
            Ok(WorkflowAgentCallDispatchOutcome::Accepted {
                target: accepted_target,
                runtime_thread_id,
                source_binding,
                ..
            }) => {
                if accepted_target != target {
                    return Ok(WorkflowAgentCallLaunchOutcome::blocked(
                        "agent_call_target_identity_drift",
                        "Product dispatch 返回了不同的 AgentRun target",
                        false,
                    ));
                }
                if let WorkflowAgentCallTargetIntent::ContinueCurrent {
                    runtime_thread_id: expected_thread,
                    source_binding: expected_binding,
                    ..
                } = &request.target_intent
                    && (&runtime_thread_id != expected_thread
                        || &source_binding != expected_binding)
                {
                    return Ok(WorkflowAgentCallLaunchOutcome::blocked(
                        "agent_call_runtime_authority_drift",
                        "Product dispatch 返回了不同的 current Runtime authority",
                        false,
                    ));
                }
                Ok(WorkflowAgentCallLaunchOutcome::Accepted {
                    target: accepted_target.clone(),
                    runtime_thread_id: runtime_thread_id.clone(),
                    dispatch_event: OrchestrationRuntimeEvent::AgentCallStarted {
                        node_path: coordinate.node_path.clone(),
                        attempt: coordinate.attempt,
                        request_id: request.identity.request_id,
                        payload_digest: request.payload_digest,
                        target: accepted_target,
                        runtime_thread_id,
                        source_binding,
                        claim_id: format!(
                            "workflow-agent-call-claim:{}:{}#{}",
                            coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
                        ),
                        timestamp: chrono::Utc::now(),
                    },
                })
            }
            Err(error) => Ok(WorkflowAgentCallLaunchOutcome::blocked(
                &error.code,
                error.message,
                error.retryable,
            )),
        }
    }

    async fn resolve_procedure(
        &self,
        run: &LifecycleRun,
        procedure: &AgentProcedureExecutionSpec,
    ) -> Result<(Option<String>, AgentProcedureContract), WorkflowApplicationError> {
        match procedure {
            AgentProcedureExecutionSpec::Snapshot {
                procedure_key,
                contract,
                ..
            } => Ok((procedure_key.clone(), contract.as_ref().clone())),
            AgentProcedureExecutionSpec::ByKey { procedure_key } => {
                let procedure = self
                    .procedure_repo
                    .get_by_project_and_key(run.project_id, procedure_key)
                    .await?
                    .ok_or_else(|| {
                        WorkflowApplicationError::NotFound(format!(
                            "AgentProcedure `{procedure_key}` 不存在"
                        ))
                    })?;
                Ok((Some(procedure_key.clone()), procedure.contract))
            }
        }
    }
}

impl WorkflowAgentCallTargetIntent {
    pub fn runtime_thread_id(&self) -> Option<&str> {
        match self {
            Self::CreateNew { .. } => None,
            Self::ContinueCurrent {
                runtime_thread_id, ..
            } => Some(runtime_thread_id),
        }
    }

    pub fn source_binding(&self) -> Option<&WorkflowAgentCallSourceBindingRef> {
        match self {
            Self::CreateNew { .. } => None,
            Self::ContinueCurrent { source_binding, .. } => Some(source_binding),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CurrentAgentAuthority {
    target: AgentRunTarget,
    runtime_thread_id: String,
    source_binding: WorkflowAgentCallSourceBindingRef,
}

enum CurrentAuthorityError {
    Missing,
    Ambiguous,
}

fn current_authority(
    run: &LifecycleRun,
    coordinate: &RuntimeNodeCoordinate,
    current_node: &RuntimeNodeState,
) -> Result<CurrentAgentAuthority, CurrentAuthorityError> {
    if let Some(history) = current_node.agent_call.as_ref() {
        if let (Some(runtime_thread_id), Some(source_binding)) =
            (&history.runtime_thread_id, &history.source_binding)
        {
            return Ok(CurrentAgentAuthority {
                target: history.target.clone(),
                runtime_thread_id: runtime_thread_id.clone(),
                source_binding: source_binding.clone(),
            });
        }
    }
    let orchestration = run
        .orchestrations
        .iter()
        .find(|item| item.orchestration_id == coordinate.orchestration_id)
        .ok_or(CurrentAuthorityError::Missing)?;
    let predecessor_ids = orchestration
        .plan_snapshot
        .activation_rules
        .iter()
        .flat_map(|rule| match rule {
            ActivationRule::Transition {
                from_node_id,
                to_node_id,
                ..
            } if to_node_id.as_str() == current_node.node_id.as_str() => vec![from_node_id.clone()],
            ActivationRule::Dependency {
                node_id,
                depends_on_node_ids,
            } if node_id.as_str() == current_node.node_id.as_str() => depends_on_node_ids.clone(),
            _ => Vec::new(),
        })
        .collect::<BTreeSet<_>>();
    let mut authorities = Vec::new();
    collect_predecessor_authorities(&orchestration.node_tree, &predecessor_ids, &mut authorities);
    authorities.sort();
    authorities.dedup();
    match authorities.as_slice() {
        [authority] => Ok(authority.clone()),
        [] => Err(CurrentAuthorityError::Missing),
        _ => Err(CurrentAuthorityError::Ambiguous),
    }
}

fn collect_predecessor_authorities(
    nodes: &[RuntimeNodeState],
    predecessor_ids: &BTreeSet<String>,
    authorities: &mut Vec<CurrentAgentAuthority>,
) {
    for node in nodes {
        if predecessor_ids.contains(&node.node_id)
            && node.started_at.is_some()
            && let Some(ExecutorRunRef::AgentRun { run_id, agent_id }) = &node.executor_run_ref
            && let Some(history) = &node.agent_call
            && history.dispatched_at.is_some()
            && let (Some(runtime_thread_id), Some(source_binding)) =
                (&history.runtime_thread_id, &history.source_binding)
        {
            authorities.push(CurrentAgentAuthority {
                target: AgentRunTarget {
                    run_id: *run_id,
                    agent_id: *agent_id,
                },
                runtime_thread_id: runtime_thread_id.clone(),
                source_binding: source_binding.clone(),
            });
        }
        collect_predecessor_authorities(&node.children, predecessor_ids, authorities);
    }
}

fn compile_input(
    contract: &AgentProcedureContract,
    node: &agentdash_domain::workflow::RuntimeNodeState,
) -> Vec<WorkflowAgentCallContentBlock> {
    let mut content = Vec::new();
    if let Some(guidance) = contract
        .injection
        .guidance
        .as_deref()
        .map(str::trim)
        .filter(|guidance| !guidance.is_empty())
    {
        content.push(WorkflowAgentCallContentBlock::Text {
            text: guidance.to_owned(),
        });
    }
    let declared = contract
        .input_ports
        .iter()
        .map(|port| port.key.as_str())
        .collect::<BTreeSet<_>>();
    for port in &contract.input_ports {
        let values = node
            .inputs
            .iter()
            .filter(|input| {
                declared.contains(input.port_key.as_str()) && input.port_key == port.key
            })
            .map(|input| input.value.clone())
            .collect::<Vec<_>>();
        let value = match values.as_slice() {
            [] => Value::Null,
            [value] => value.clone(),
            _ => Value::Array(values),
        };
        content.push(WorkflowAgentCallContentBlock::Structured {
            schema: WORKFLOW_AGENT_CALL_INPUT_PORT_SCHEMA_V1.to_owned(),
            value: json!({
                "port_key": port.key,
                "value": value,
            }),
        });
    }
    content
}

pub(super) enum WorkflowAgentCallLaunchOutcome {
    Prepared {
        event: OrchestrationRuntimeEvent,
    },
    Pending,
    Accepted {
        target: AgentRunTarget,
        runtime_thread_id: String,
        dispatch_event: OrchestrationRuntimeEvent,
    },
    Blocked {
        code: String,
        message: String,
        retryable: bool,
    },
}

impl WorkflowAgentCallLaunchOutcome {
    fn blocked(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self::Blocked {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::{
        ContextStrategy, InputPortDefinition, NodePortValue, PlanNodeKind, RuntimeNodeState,
        RuntimeNodeStatus, StandaloneFulfillment,
    };

    #[test]
    fn guidance_and_all_declared_inputs_are_lossless_typed_blocks() {
        let contract = AgentProcedureContract {
            injection: agentdash_domain::workflow::WorkflowInjectionSpec {
                guidance: Some("follow the procedure".to_owned()),
                ..Default::default()
            },
            input_ports: ["prompt", "context"]
                .into_iter()
                .map(|key| InputPortDefinition {
                    key: key.to_owned(),
                    description: String::new(),
                    context_strategy: ContextStrategy::Full,
                    context_template: None,
                    standalone_fulfillment: StandaloneFulfillment::Required,
                })
                .collect(),
            ..Default::default()
        };
        let node = RuntimeNodeState {
            node_id: "node".to_owned(),
            node_path: "node".to_owned(),
            kind: PlanNodeKind::AgentCall,
            status: RuntimeNodeStatus::Ready,
            attempt: 1,
            inputs: vec![
                NodePortValue {
                    port_key: "prompt".to_owned(),
                    value: json!("ship it"),
                },
                NodePortValue {
                    port_key: "context".to_owned(),
                    value: json!({"nested":[1, true, null]}),
                },
            ],
            outputs: Vec::new(),
            executor_run_ref: None,
            agent_call: None,
            children: Vec::new(),
            phase_path: Vec::new(),
            started_at: None,
            completed_at: None,
            error: None,
            trace_refs: Vec::new(),
            cache: None,
        };

        assert_eq!(
            compile_input(&contract, &node),
            vec![
                WorkflowAgentCallContentBlock::Text {
                    text: "follow the procedure".to_owned(),
                },
                WorkflowAgentCallContentBlock::Structured {
                    schema: WORKFLOW_AGENT_CALL_INPUT_PORT_SCHEMA_V1.to_owned(),
                    value: json!({"port_key":"prompt","value":"ship it"}),
                },
                WorkflowAgentCallContentBlock::Structured {
                    schema: WORKFLOW_AGENT_CALL_INPUT_PORT_SCHEMA_V1.to_owned(),
                    value: json!({"port_key":"context","value":{"nested":[1,true,null]}}),
                },
            ]
        );
    }
}
