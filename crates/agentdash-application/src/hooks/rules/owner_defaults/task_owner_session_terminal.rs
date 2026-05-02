use agentdash_domain::workflow::{WorkflowHookRuleSpec, WorkflowHookTrigger};
use agentdash_spi::SessionHookSnapshot;

pub(super) const REGISTRY_ITEM: fn(&SessionHookSnapshot) -> Option<WorkflowHookRuleSpec> =
    build_rule;

fn build_rule(snapshot: &SessionHookSnapshot) -> Option<WorkflowHookRuleSpec> {
    if !has_task_owner(snapshot) {
        return None;
    }
    Some(WorkflowHookRuleSpec {
        key: "builtin:task_session_terminal".to_string(),
        trigger: WorkflowHookTrigger::SessionTerminal,
        description: "Task 默认 lifecycle: session 终止时根据 terminal_state 转换 task 状态"
            .to_string(),
        preset: Some("task_session_terminal".to_string()),
        params: None,
        script: None,
        enabled: true,
    })
}

fn has_task_owner(snapshot: &SessionHookSnapshot) -> bool {
    snapshot.owners.iter().any(|o| {
        o.owner_type == agentdash_domain::session_binding::SessionOwnerType::Task
            && o.task_id.is_some()
    })
}
