use agentdash_domain::workflow::{
    AgentAssignment, AgentAssignmentRepository, AgentFrame, AgentFrameRepository,
    LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
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

/// RuntimeSession terminal / advance 事件解析出的稳定 lifecycle execution 证据。
///
/// `assignment.frame_id` 表达 activity attempt 启动时的 launch frame evidence。
/// resolver 必须允许 runtime session 的当前 frame revision 演进后仍能回到 assignment。
#[derive(Debug, Clone)]
pub struct ActivityRuntimeAssociation {
    pub run: LifecycleRun,
    pub assignment: AgentAssignment,
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

pub struct ActivityRuntimeAssociationResolver<'a> {
    frame_repo: &'a dyn AgentFrameRepository,
    agent_repo: &'a dyn LifecycleAgentRepository,
    assignment_repo: &'a dyn AgentAssignmentRepository,
    run_repo: &'a dyn LifecycleRunRepository,
}

impl<'a> ActivityRuntimeAssociationResolver<'a> {
    pub fn new(
        frame_repo: &'a dyn AgentFrameRepository,
        agent_repo: &'a dyn LifecycleAgentRepository,
        assignment_repo: &'a dyn AgentAssignmentRepository,
        run_repo: &'a dyn LifecycleRunRepository,
    ) -> Self {
        Self {
            frame_repo,
            agent_repo,
            assignment_repo,
            run_repo,
        }
    }

    pub async fn resolve_by_runtime_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ActivityRuntimeAssociation>, String> {
        let Some(current_frame) = self
            .frame_repo
            .find_by_runtime_session(session_id)
            .await
            .map_err(|e| format!("查询 runtime session 对应 AgentFrame 失败: {e}"))?
        else {
            return Ok(None);
        };
        let Some(agent) = self
            .agent_repo
            .get(current_frame.agent_id)
            .await
            .map_err(|e| format!("查询 lifecycle agent 失败: {e}"))?
        else {
            return Ok(None);
        };
        let assignments = self
            .assignment_repo
            .list_by_run(agent.run_id)
            .await
            .map_err(|e| format!("查询 agent assignments 失败: {e}"))?;
        let Some(assignment) = select_assignment_for_runtime_frame(&assignments, &current_frame)?
        else {
            return Ok(None);
        };
        let attempt = u32::try_from(assignment.attempt)
            .map_err(|_| format!("agent assignment attempt 无效: {}", assignment.attempt))?;
        let run = self
            .run_repo
            .get_by_id(assignment.run_id)
            .await
            .map_err(|e| format!("查询 lifecycle run 失败: {e}"))?
            .ok_or_else(|| format!("lifecycle run 不存在: {}", assignment.run_id))?;
        Ok(Some(ActivityRuntimeAssociation {
            run,
            attempt,
            assignment,
        }))
    }
}

pub(crate) fn select_assignment_for_runtime_frame(
    assignments: &[AgentAssignment],
    frame: &AgentFrame,
) -> Result<Option<AgentAssignment>, String> {
    let active_for_agent = assignments
        .iter()
        .filter(|assignment| assignment.lease_status == "active")
        .filter(|assignment| assignment.agent_id == frame.agent_id)
        .collect::<Vec<_>>();

    if let Some(assignment) = active_for_agent
        .iter()
        .find(|assignment| assignment.frame_id == frame.id)
    {
        return Ok(Some((*assignment).clone()));
    }

    if let (Some(graph_instance_id), Some(activity_key)) =
        (frame.graph_instance_id, frame.activity_key.as_deref())
    {
        let scoped = active_for_agent
            .iter()
            .filter(|assignment| {
                assignment.graph_instance_id == graph_instance_id
                    && assignment.activity_key == activity_key
            })
            .collect::<Vec<_>>();
        match scoped.as_slice() {
            [assignment] => return Ok(Some((**assignment).clone())),
            [] => {}
            _ => {
                return Err(format!(
                    "runtime session 对应 AgentFrame 匹配到多个 active assignments: agent_id={}, graph_instance_id={}, activity_key={activity_key}",
                    frame.agent_id, graph_instance_id
                ));
            }
        }
    }

    match active_for_agent.as_slice() {
        [assignment] => Ok(Some((**assignment).clone())),
        [] => Ok(None),
        _ => Err(format!(
            "runtime session 对应 AgentFrame 缺少 graph/activity scope，且 agent 存在多个 active assignments: agent_id={}",
            frame.agent_id
        )),
    }
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
    let resolver =
        ActivityRuntimeAssociationResolver::new(frame_repo, agent_repo, assignment_repo, run_repo);
    Ok(resolver
        .resolve_by_runtime_session(session_id)
        .await?
        .map(|association| {
            let ActivityRuntimeAssociation {
                run,
                assignment,
                attempt,
            } = association;
            LifecycleActivitySessionAssociation {
                run,
                activity_key: assignment.activity_key,
                attempt,
            }
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::AgentFrame;

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

    #[test]
    fn selects_exact_launch_frame_assignment_first() {
        let run_id = Uuid::new_v4();
        let graph_instance_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_revision(agent_id, 1, "launch");
        let exact = AgentAssignment::new(
            run_id,
            graph_instance_id,
            "implement",
            1,
            agent_id,
            frame.id,
        );
        let other = AgentAssignment::new(
            run_id,
            graph_instance_id,
            "review",
            1,
            agent_id,
            Uuid::new_v4(),
        );

        let selected = select_assignment_for_runtime_frame(&[other, exact.clone()], &frame)
            .expect("selection should not error")
            .expect("assignment should resolve");

        assert_eq!(selected.id, exact.id);
    }

    #[test]
    fn selects_assignment_by_current_frame_activity_scope_after_revision_change() {
        let run_id = Uuid::new_v4();
        let graph_instance_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let launch_frame_id = Uuid::new_v4();
        let mut current_frame = AgentFrame::new_revision(agent_id, 2, "capability_update");
        current_frame.graph_instance_id = Some(graph_instance_id);
        current_frame.activity_key = Some("implement".to_string());
        let assignment = AgentAssignment::new(
            run_id,
            graph_instance_id,
            "implement",
            1,
            agent_id,
            launch_frame_id,
        );

        let selected = select_assignment_for_runtime_frame(&[assignment.clone()], &current_frame)
            .expect("selection should not error")
            .expect("assignment should resolve through graph/activity scope");

        assert_eq!(selected.id, assignment.id);
        assert_eq!(selected.frame_id, launch_frame_id);
    }

    #[test]
    fn rejects_ambiguous_assignments_when_current_frame_lacks_activity_scope() {
        let run_id = Uuid::new_v4();
        let graph_instance_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_revision(agent_id, 2, "capability_update");
        let a1 = AgentAssignment::new(
            run_id,
            graph_instance_id,
            "implement",
            1,
            agent_id,
            Uuid::new_v4(),
        );
        let a2 = AgentAssignment::new(
            run_id,
            graph_instance_id,
            "review",
            1,
            agent_id,
            Uuid::new_v4(),
        );

        let error = select_assignment_for_runtime_frame(&[a1, a2], &frame)
            .expect_err("ambiguous assignments should be rejected");

        assert!(error.contains("多个 active assignments"));
    }

    #[test]
    fn rejects_ambiguous_assignments_in_same_frame_activity_scope() {
        let run_id = Uuid::new_v4();
        let graph_instance_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let mut frame = AgentFrame::new_revision(agent_id, 2, "capability_update");
        frame.graph_instance_id = Some(graph_instance_id);
        frame.activity_key = Some("implement".to_string());
        let a1 = AgentAssignment::new(
            run_id,
            graph_instance_id,
            "implement",
            1,
            agent_id,
            Uuid::new_v4(),
        );
        let a2 = AgentAssignment::new(
            run_id,
            graph_instance_id,
            "implement",
            2,
            agent_id,
            Uuid::new_v4(),
        );

        let error = select_assignment_for_runtime_frame(&[a1, a2], &frame)
            .expect_err("ambiguous scoped assignments should be rejected");

        assert!(error.contains("匹配到多个 active assignments"));
    }
}
