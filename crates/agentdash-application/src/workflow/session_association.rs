use agentdash_domain::workflow::{
    AgentAssignmentRepository, AgentFrameRepository, LifecycleAgentRepository, LifecycleRun,
    LifecycleRunRepository,
};
use uuid::Uuid;

/// Lifecycle node 子 session 的 binding label 前缀。
pub const LIFECYCLE_NODE_LABEL_PREFIX: &str = "lifecycle_node:";
pub const LIFECYCLE_ACTIVITY_LABEL_PREFIX: &str = "lifecycle_activity:";

/// 子 session 与 lifecycle activity attempt 的关联解析结果。
#[derive(Debug, Clone)]
pub struct LifecycleActivitySessionAssociation {
    pub run: LifecycleRun,
    pub activity_key: String,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleActivityLabelParts {
    pub run_id: Uuid,
    pub activity_key: String,
    pub attempt: u32,
}

/// 构造 lifecycle node 子 session 的 binding label。
pub fn build_lifecycle_node_label(node_key: &str) -> String {
    format!("{LIFECYCLE_NODE_LABEL_PREFIX}{node_key}")
}

pub fn build_lifecycle_activity_label(run_id: Uuid, activity_key: &str, attempt: u32) -> String {
    format!("{LIFECYCLE_ACTIVITY_LABEL_PREFIX}{run_id}:{activity_key}#{attempt}")
}

pub fn lifecycle_activity_parts_from_label(label: &str) -> Option<LifecycleActivityLabelParts> {
    let payload = label.strip_prefix(LIFECYCLE_ACTIVITY_LABEL_PREFIX)?.trim();
    let (run_id, activity_attempt) = payload.split_once(':')?;
    let (activity_key, attempt) = activity_attempt.rsplit_once('#')?;
    if activity_key.is_empty() {
        return None;
    }
    Some(LifecycleActivityLabelParts {
        run_id: Uuid::parse_str(run_id).ok()?,
        activity_key: activity_key.to_string(),
        attempt: attempt.parse().ok()?,
    })
}

/// 解析 session 是否为某个 lifecycle activity attempt 的执行 session。
///
/// 通过 RuntimeSession -> AgentFrame -> LifecycleAgent -> AgentAssignment 反查。
pub async fn resolve_activity_session_association(
    session_id: &str,
    frame_repo: &dyn AgentFrameRepository,
    agent_repo: &dyn LifecycleAgentRepository,
    assignment_repo: &dyn AgentAssignmentRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<LifecycleActivitySessionAssociation>, String> {
    let Some(frame) = frame_repo
        .find_by_runtime_session(session_id)
        .await
        .map_err(|e| format!("查询 runtime session 对应 AgentFrame 失败: {e}"))?
    else {
        return Ok(None);
    };
    let Some(agent) = agent_repo
        .get(frame.agent_id)
        .await
        .map_err(|e| format!("查询 lifecycle agent 失败: {e}"))?
    else {
        return Ok(None);
    };
    let assignments = assignment_repo
        .list_by_run(agent.run_id)
        .await
        .map_err(|e| format!("查询 agent assignments 失败: {e}"))?;
    let Some(assignment) = assignments
        .into_iter()
        .filter(|assignment| assignment.lease_status == "active")
        .find(|assignment| assignment.frame_id == frame.id && assignment.agent_id == agent.id)
    else {
        return Ok(None);
    };
    let run = run_repo
        .get_by_id(assignment.run_id)
        .await
        .map_err(|e| format!("查询 lifecycle run 失败: {e}"))?
        .ok_or_else(|| format!("lifecycle run 不存在: {}", assignment.run_id))?;
    Ok(Some(LifecycleActivitySessionAssociation {
        run,
        activity_key: assignment.activity_key,
        attempt: assignment.attempt as u32,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_activity_label_roundtrips_run_activity_and_attempt() {
        let run_id = Uuid::new_v4();
        let label = build_lifecycle_activity_label(run_id, "plan", 2);

        assert_eq!(
            lifecycle_activity_parts_from_label(&label),
            Some(LifecycleActivityLabelParts {
                run_id,
                activity_key: "plan".to_string(),
                attempt: 2,
            })
        );
    }

    #[test]
    fn lifecycle_activity_label_rejects_incomplete_payload() {
        assert_eq!(
            lifecycle_activity_parts_from_label("lifecycle_activity:plan#1"),
            None
        );
        assert_eq!(
            lifecycle_activity_parts_from_label("lifecycle_activity:not-a-uuid:plan#1"),
            None
        );
        assert_eq!(
            lifecycle_activity_parts_from_label(
                "lifecycle_activity:00000000-0000-0000-0000-000000000000:plan"
            ),
            None
        );
    }
}
