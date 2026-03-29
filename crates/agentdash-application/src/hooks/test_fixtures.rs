use agentdash_domain::workflow::{
    WorkflowCheckKind, WorkflowCheckSpec, WorkflowCompletionSpec, WorkflowConstraintKind,
    WorkflowContract,
};
use agentdash_spi::{
    ActiveWorkflowMeta, HookSourceLayer, HookSourceRef, SessionHookSnapshot,
    SessionSnapshotMetadata,
};

pub fn snapshot_with_workflow(step_key: &str, completion_mode: &str) -> SessionHookSnapshot {
    snapshot_with_workflow_and_evidence(step_key, completion_mode, false)
}

pub fn snapshot_with_workflow_and_evidence(
    step_key: &str,
    completion_mode: &str,
    checklist_evidence_present: bool,
) -> SessionHookSnapshot {
    let (step_advance, workflow_key, mut contract) = match completion_mode {
        "checklist_passed" => (
            "auto",
            Some("trellis_dev_task_check"),
            WorkflowContract {
                constraints: vec![agentdash_domain::workflow::WorkflowConstraintSpec {
                    key: "block_stop_until_checks_pass".to_string(),
                    kind: WorkflowConstraintKind::BlockStopUntilChecksPass,
                    description: "block stop".to_string(),
                    payload: None,
                }],
                completion: WorkflowCompletionSpec {
                    checks: vec![WorkflowCheckSpec {
                        key: "checklist_evidence_present".to_string(),
                        kind: WorkflowCheckKind::ChecklistEvidencePresent,
                        description: "checklist evidence".to_string(),
                        payload: None,
                    }],
                    ..WorkflowCompletionSpec::default()
                },
                ..WorkflowContract::default()
            },
        ),
        "session_ended" => (
            "auto",
            Some("trellis_dev_task_implement"),
            WorkflowContract::default(),
        ),
        _ => ("manual", None, WorkflowContract::default()),
    };
    let effective_contract = serde_json::json!(contract);
    let workflow_source = HookSourceRef {
        layer: HookSourceLayer::Workflow,
        key: format!("trellis_dev_task:{step_key}"),
        label: format!("Workflow / Trellis Dev Workflow / {step_key}"),
        priority: 300,
    };
    let mut snapshot = SessionHookSnapshot {
        session_id: "sess-test".to_string(),
        sources: vec![workflow_source],
        metadata: Some(SessionSnapshotMetadata {
            workspace_root: Some("F:/Projects/AgentDash".to_string()),
            active_workflow: Some(ActiveWorkflowMeta {
                lifecycle_key: Some("trellis_dev_task".to_string()),
                step_key: Some(step_key.to_string()),
                step_advance: Some(step_advance.to_string()),
                transition_policy: Some(step_advance.to_string()),
                workflow_key: workflow_key.map(str::to_string),
                run_id: Some("00000000-0000-0000-0000-0000000000aa".to_string()),
                effective_contract: Some(effective_contract),
                checklist_evidence_present: Some(checklist_evidence_present),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..SessionHookSnapshot::default()
    };
    snapshot
}

pub fn snapshot_with_supervised_policy() -> SessionHookSnapshot {
    SessionHookSnapshot {
        session_id: "sess-supervised".to_string(),
        metadata: Some(SessionSnapshotMetadata {
            workspace_root: Some("F:/Projects/AgentDash".to_string()),
            permission_policy: Some("SUPERVISED".to_string()),
            ..Default::default()
        }),
        ..SessionHookSnapshot::default()
    }
}
