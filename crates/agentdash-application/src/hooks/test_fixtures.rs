use agentdash_domain::workflow::{WorkflowContract, WorkflowHookRuleSpec, WorkflowHookTrigger};
use agentdash_spi::{ActiveWorkflowMeta, SessionHookSnapshot, SessionSnapshotMetadata};

pub fn snapshot_with_workflow(step_key: &str, completion_mode: &str) -> SessionHookSnapshot {
    snapshot_with_workflow_ports(step_key, completion_mode, &[], &[])
}

/// 构建带 output port 配置的 workflow snapshot，用于 port_output_gate 测试。
pub fn snapshot_with_workflow_ports(
    step_key: &str,
    completion_mode: &str,
    output_port_keys: &[&str],
    fulfilled_port_keys: &[&str],
) -> SessionHookSnapshot {
    let (transition_policy, workflow_key, contract) = match completion_mode {
        "checklist_passed" => (
            "auto",
            Some("trellis_dev_task_check"),
            WorkflowContract {
                hook_rules: vec![WorkflowHookRuleSpec {
                    key: "port_output_gate".to_string(),
                    trigger: WorkflowHookTrigger::BeforeStop,
                    description: "port output gate".to_string(),
                    preset: Some("port_output_gate".to_string()),
                    params: None,
                    script: None,
                    enabled: true,
                }],
                ..WorkflowContract::default()
            },
        ),
        "session_ended" => (
            "auto",
            Some("trellis_dev_task_implement"),
            WorkflowContract {
                hook_rules: vec![WorkflowHookRuleSpec {
                    key: "terminal_advance".to_string(),
                    trigger: WorkflowHookTrigger::BeforeStop,
                    description: "advance on terminal".to_string(),
                    preset: Some("session_terminal_advance".to_string()),
                    params: None,
                    script: None,
                    enabled: true,
                }],
                ..WorkflowContract::default()
            },
        ),
        _ => ("manual", None, WorkflowContract::default()),
    };
    let effective_contract = agentdash_domain::workflow::EffectiveSessionContract {
        injection: contract.injection,
        hook_rules: contract.hook_rules,
        ..Default::default()
    };
    let workflow_source = format!("workflow:trellis_dev_task:{step_key}");
    let port_keys_opt = if output_port_keys.is_empty() {
        None
    } else {
        Some(output_port_keys.iter().map(|k| k.to_string()).collect())
    };
    let fulfilled_opt = if fulfilled_port_keys.is_empty() {
        None
    } else {
        Some(fulfilled_port_keys.iter().map(|k| k.to_string()).collect())
    };
    SessionHookSnapshot {
        session_id: "sess-test".to_string(),
        sources: vec![workflow_source],
        metadata: Some(SessionSnapshotMetadata {
            active_workflow: Some(ActiveWorkflowMeta {
                lifecycle_key: Some("trellis_dev_task".to_string()),
                step_key: Some(step_key.to_string()),
                transition_policy: Some(transition_policy.to_string()),
                workflow_key: workflow_key.map(str::to_string),
                run_id: Some(
                    uuid::Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").unwrap(),
                ),
                effective_contract: Some(effective_contract),
                node_type: Some("agent_node".to_string()),
                output_port_keys: port_keys_opt,
                fulfilled_port_keys: fulfilled_opt,
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..SessionHookSnapshot::default()
    }
}

pub fn snapshot_with_supervised_policy() -> SessionHookSnapshot {
    SessionHookSnapshot {
        session_id: "sess-supervised".to_string(),
        metadata: Some(SessionSnapshotMetadata {
            permission_policy: Some("SUPERVISED".to_string()),
            ..Default::default()
        }),
        ..SessionHookSnapshot::default()
    }
}
