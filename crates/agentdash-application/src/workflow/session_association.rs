use agentdash_domain::workflow::{
    AgentAssignment, AgentAssignmentRepository, AgentFrame, AgentFrameRepository, LifecycleAgent,
    LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository,
};
use uuid::Uuid;

/// Lifecycle node 子 session 的 binding label 前缀。
pub const LIFECYCLE_NODE_LABEL_PREFIX: &str = "lifecycle_node:";
pub const LIFECYCLE_ACTIVITY_LABEL_PREFIX: &str = "lifecycle_activity:";

/// 子 session 与 lifecycle activity attempt 的关联解析结果。
#[derive(Debug, Clone)]
pub struct LifecycleActivitySessionAssociation {
    pub run: LifecycleRun,
    pub graph_instance_id: Uuid,
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

pub type RuntimeSessionCurrentFrame = (RuntimeSessionExecutionAnchor, LifecycleAgent, AgentFrame);

pub async fn resolve_current_frame_for_runtime_session(
    runtime_session_id: &str,
    anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
    agent_repo: &dyn LifecycleAgentRepository,
    frame_repo: &dyn AgentFrameRepository,
) -> Result<Option<RuntimeSessionCurrentFrame>, agentdash_domain::DomainError> {
    let Some(anchor) = anchor_repo.find_by_session(runtime_session_id).await? else {
        return Ok(None);
    };
    let Some(agent) = agent_repo.get(anchor.agent_id).await? else {
        return Ok(None);
    };
    if agent.run_id != anchor.run_id {
        return Ok(None);
    }
    let frame = match frame_repo.get_current(agent.id).await? {
        Some(frame) => frame,
        None => match frame_repo.get(anchor.launch_frame_id).await? {
            Some(frame) => frame,
            None => return Ok(None),
        },
    };
    if frame.agent_id != agent.id {
        return Ok(None);
    }
    Ok(Some((anchor, agent, frame)))
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActivityRuntimeAssociationError {
    #[error("查询 {operation} 失败: {message}")]
    Repository {
        operation: &'static str,
        message: String,
    },
    #[error(
        "runtime session {runtime_session_id} 对应 AgentFrame {frame_id} 缺少 LifecycleAgent: agent_id={agent_id}"
    )]
    MissingLifecycleAgent {
        runtime_session_id: String,
        frame_id: Uuid,
        agent_id: Uuid,
    },
    #[error(
        "runtime session {runtime_session_id} 对应 AgentFrame {frame_id} 没有关联 active AgentAssignment: agent_id={agent_id}"
    )]
    MissingAssignment {
        runtime_session_id: String,
        frame_id: Uuid,
        agent_id: Uuid,
    },
    #[error(
        "AgentFrame {frame_id} 对应多个 active AgentAssignment: agent_id={agent_id}, graph_instance_id={graph_instance_id:?}, activity_key={activity_key:?}"
    )]
    AmbiguousAssignments {
        frame_id: Uuid,
        agent_id: Uuid,
        graph_instance_id: Option<Uuid>,
        activity_key: Option<String>,
    },
    #[error("AgentAssignment {assignment_id} attempt 无效: {attempt}")]
    InvalidAttempt { assignment_id: Uuid, attempt: i32 },
    #[error("AgentAssignment {assignment_id} 指向的 lifecycle run 不存在: {run_id}")]
    MissingLifecycleRun { assignment_id: Uuid, run_id: Uuid },
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
    assignment_repo: &'a dyn AgentAssignmentRepository,
    run_repo: &'a dyn LifecycleRunRepository,
    anchor_repo: Option<&'a dyn RuntimeSessionExecutionAnchorRepository>,
}

impl<'a> ActivityRuntimeAssociationResolver<'a> {
    pub fn new(
        frame_repo: &'a dyn AgentFrameRepository,
        assignment_repo: &'a dyn AgentAssignmentRepository,
        run_repo: &'a dyn LifecycleRunRepository,
    ) -> Self {
        Self {
            frame_repo,
            assignment_repo,
            run_repo,
            anchor_repo: None,
        }
    }

    pub fn with_anchor_repo(
        mut self,
        anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    ) -> Self {
        self.anchor_repo = Some(anchor_repo);
        self
    }

    pub async fn resolve_by_runtime_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ActivityRuntimeAssociation>, ActivityRuntimeAssociationError> {
        let Some(anchor_repo) = self.anchor_repo else {
            return Ok(None);
        };
        let Some(anchor) = anchor_repo.find_by_session(session_id).await.map_err(|e| {
            ActivityRuntimeAssociationError::Repository {
                operation: "runtime session execution anchor",
                message: e.to_string(),
            }
        })?
        else {
            return Ok(None);
        };
        self.resolve_by_anchor(session_id, anchor).await
    }

    async fn resolve_by_anchor(
        &self,
        session_id: &str,
        anchor: RuntimeSessionExecutionAnchor,
    ) -> Result<Option<ActivityRuntimeAssociation>, ActivityRuntimeAssociationError> {
        let Some(assignment_id) = anchor.assignment_id else {
            let Some(frame) = self
                .frame_repo
                .get(anchor.launch_frame_id)
                .await
                .map_err(|e| ActivityRuntimeAssociationError::Repository {
                    operation: "anchor launch AgentFrame",
                    message: e.to_string(),
                })?
            else {
                return Ok(None);
            };
            if frame.graph_instance_id.is_none() && frame.activity_key.is_none() {
                return Ok(None);
            }
            return Err(ActivityRuntimeAssociationError::MissingAssignment {
                runtime_session_id: session_id.to_string(),
                frame_id: frame.id,
                agent_id: frame.agent_id,
            });
        };

        let assignment = self
            .assignment_repo
            .get(assignment_id)
            .await
            .map_err(|e| ActivityRuntimeAssociationError::Repository {
                operation: "anchor AgentAssignment",
                message: e.to_string(),
            })?
            .ok_or_else(|| ActivityRuntimeAssociationError::MissingAssignment {
                runtime_session_id: session_id.to_string(),
                frame_id: anchor.launch_frame_id,
                agent_id: anchor.agent_id,
            })?;

        if assignment.run_id != anchor.run_id
            || assignment.agent_id != anchor.agent_id
            || assignment.frame_id != anchor.launch_frame_id
        {
            return Err(ActivityRuntimeAssociationError::MissingAssignment {
                runtime_session_id: session_id.to_string(),
                frame_id: anchor.launch_frame_id,
                agent_id: anchor.agent_id,
            });
        }

        let attempt =
            u32::try_from(anchor.attempt.unwrap_or(assignment.attempt)).map_err(|_| {
                ActivityRuntimeAssociationError::InvalidAttempt {
                    assignment_id: assignment.id,
                    attempt: assignment.attempt,
                }
            })?;
        let run = self
            .run_repo
            .get_by_id(assignment.run_id)
            .await
            .map_err(|e| ActivityRuntimeAssociationError::Repository {
                operation: "lifecycle run",
                message: e.to_string(),
            })?
            .ok_or(ActivityRuntimeAssociationError::MissingLifecycleRun {
                assignment_id: assignment.id,
                run_id: assignment.run_id,
            })?;
        Ok(Some(ActivityRuntimeAssociation {
            run,
            attempt,
            assignment,
        }))
    }
}

/// 从 agent 的 active assignments 中精确匹配 frame 对应的 assignment。
///
/// 查询路径: `find_active_for_agent(frame.agent_id)` -> 按 frame_id 精确匹配
/// -> 按 graph_instance_id + activity_key 精确匹配 -> 无 fallback。
pub(crate) async fn select_assignment_for_frame(
    assignment_repo: &dyn AgentAssignmentRepository,
    frame: &AgentFrame,
) -> Result<Option<AgentAssignment>, ActivityRuntimeAssociationError> {
    let active_for_agent = assignment_repo
        .find_active_for_agent(frame.agent_id)
        .await
        .map_err(|e| ActivityRuntimeAssociationError::Repository {
            operation: "agent active assignments",
            message: e.to_string(),
        })?;

    if let Some(assignment) = active_for_agent.iter().find(|a| a.frame_id == frame.id) {
        return Ok(Some(assignment.clone()));
    }

    if let (Some(graph_instance_id), Some(activity_key)) =
        (frame.graph_instance_id, frame.activity_key.as_deref())
    {
        let scoped: Vec<_> = active_for_agent
            .iter()
            .filter(|a| a.graph_instance_id == graph_instance_id && a.activity_key == activity_key)
            .collect();
        match scoped.as_slice() {
            [assignment] => return Ok(Some((*assignment).clone())),
            [] => return Ok(None),
            _ => {
                return Err(ActivityRuntimeAssociationError::AmbiguousAssignments {
                    frame_id: frame.id,
                    agent_id: frame.agent_id,
                    graph_instance_id: Some(graph_instance_id),
                    activity_key: Some(activity_key.to_string()),
                });
            }
        }
    }

    Ok(None)
}

/// 仅测试用：从内存中的 assignment 列表和 frame 做启发式选择。
/// 生产代码应使用 [`select_assignment_for_frame`]。
#[cfg(test)]
pub(crate) fn select_assignment_for_runtime_frame(
    assignments: &[AgentAssignment],
    frame: &AgentFrame,
) -> Result<Option<AgentAssignment>, ActivityRuntimeAssociationError> {
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
                return Err(ActivityRuntimeAssociationError::AmbiguousAssignments {
                    frame_id: frame.id,
                    agent_id: frame.agent_id,
                    graph_instance_id: Some(graph_instance_id),
                    activity_key: Some(activity_key.to_string()),
                });
            }
        }
    }

    match active_for_agent.as_slice() {
        [assignment] => Ok(Some((**assignment).clone())),
        [] => Ok(None),
        _ => Err(ActivityRuntimeAssociationError::AmbiguousAssignments {
            frame_id: frame.id,
            agent_id: frame.agent_id,
            graph_instance_id: None,
            activity_key: None,
        }),
    }
}

/// 解析 session 是否为某个 lifecycle activity attempt 的执行 session。
///
/// 直接锚定路径: RuntimeSession -> AgentFrame -> find_active_for_agent -> Assignment -> Run。
pub async fn resolve_activity_session_association(
    session_id: &str,
    frame_repo: &dyn AgentFrameRepository,
    _agent_repo: &dyn LifecycleAgentRepository,
    assignment_repo: &dyn AgentAssignmentRepository,
    run_repo: &dyn LifecycleRunRepository,
    anchor_repo: Option<&dyn RuntimeSessionExecutionAnchorRepository>,
) -> Result<Option<LifecycleActivitySessionAssociation>, ActivityRuntimeAssociationError> {
    let mut resolver =
        ActivityRuntimeAssociationResolver::new(frame_repo, assignment_repo, run_repo);
    if let Some(anchor_repo) = anchor_repo {
        resolver = resolver.with_anchor_repo(anchor_repo);
    }
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
                graph_instance_id: assignment.graph_instance_id,
                activity_key: assignment.activity_key,
                attempt,
            }
        }))
}

#[cfg(test)]
mod tests {
    #[allow(deprecated)]
    use super::select_assignment_for_runtime_frame;
    use super::*;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        AgentAssignmentRepository, AgentFrame, AgentFrameRepository, LifecycleRunRepository,
    };
    use std::collections::HashMap;

    #[derive(Default)]
    struct TestFrameRepo {
        frames: HashMap<Uuid, AgentFrame>,
    }

    #[async_trait::async_trait]
    impl AgentFrameRepository for TestFrameRepo {
        async fn create(&self, _frame: &AgentFrame) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self.frames.get(&frame_id).cloned())
        }

        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .values()
                .filter(|frame| frame.agent_id == agent_id)
                .max_by_key(|frame| frame.revision)
                .cloned())
        }

        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .values()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn append_visible_canvas_mount(
            &self,
            _frame_id: Uuid,
            _mount_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct TestAssignmentRepo {
        assignments: Vec<AgentAssignment>,
    }

    #[async_trait::async_trait]
    impl AgentAssignmentRepository for TestAssignmentRepo {
        async fn create(&self, _assignment: &AgentAssignment) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get(&self, assignment_id: Uuid) -> Result<Option<AgentAssignment>, DomainError> {
            Ok(self
                .assignments
                .iter()
                .find(|assignment| assignment.id == assignment_id)
                .cloned())
        }

        async fn find_for_attempt(
            &self,
            graph_instance_id: Uuid,
            activity_key: &str,
            attempt: i32,
        ) -> Result<Option<AgentAssignment>, DomainError> {
            Ok(self
                .assignments
                .iter()
                .find(|assignment| {
                    assignment.graph_instance_id == graph_instance_id
                        && assignment.activity_key == activity_key
                        && assignment.attempt == attempt
                })
                .cloned())
        }

        async fn find_active_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<AgentAssignment>, DomainError> {
            Ok(self
                .assignments
                .iter()
                .filter(|a| a.agent_id == agent_id && a.lease_status == "active")
                .cloned()
                .collect())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentAssignment>, DomainError> {
            Ok(self
                .assignments
                .iter()
                .filter(|assignment| assignment.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, _assignment: &AgentAssignment) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct TestRunRepo {
        runs: HashMap<Uuid, LifecycleRun>,
    }

    #[async_trait::async_trait]
    impl LifecycleRunRepository for TestRunRepo {
        async fn create(&self, _run: &LifecycleRun) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self.runs.get(&id).cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(ids
                .iter()
                .filter_map(|id| self.runs.get(id).cloned())
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .values()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_by_root_graph(
            &self,
            root_graph_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .values()
                .filter(|run| run.root_graph_id == Some(root_graph_id))
                .cloned()
                .collect())
        }

        async fn update(&self, _run: &LifecycleRun) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct TestAnchorRepo {
        anchors: HashMap<String, RuntimeSessionExecutionAnchor>,
    }

    #[async_trait::async_trait]
    impl RuntimeSessionExecutionAnchorRepository for TestAnchorRepo {
        async fn upsert(&self, _anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
            Ok(())
        }

        async fn update_assignment(
            &self,
            _runtime_session_id: &str,
            _assignment_id: Uuid,
            _attempt: i32,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete_by_session(&self, _runtime_session_id: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self.anchors.get(runtime_session_id).cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .values()
                .filter(|anchor| anchor.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .values()
                .filter(|anchor| anchor.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(runtime_session_ids
                .iter()
                .filter_map(|id| self.anchors.get(id).cloned())
                .collect())
        }

        async fn latest_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .values()
                .filter(|anchor| anchor.agent_id == agent_id)
                .max_by_key(|anchor| anchor.updated_at)
                .cloned())
        }
    }

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

        assert!(matches!(
            error,
            ActivityRuntimeAssociationError::AmbiguousAssignments {
                graph_instance_id: None,
                activity_key: None,
                ..
            }
        ));
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

        assert!(matches!(
            error,
            ActivityRuntimeAssociationError::AmbiguousAssignments {
                graph_instance_id: Some(id),
                activity_key: Some(ref key),
                ..
            } if id == graph_instance_id && key == "implement"
        ));
    }

    #[tokio::test]
    async fn resolver_keeps_assignment_after_runtime_frame_revision_changes() {
        let project_id = Uuid::new_v4();
        let lifecycle_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let graph_instance_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let launch_frame_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id, lifecycle_id);
        run.id = run_id;
        let mut current_frame = AgentFrame::new_revision(agent_id, 2, "capability_update");
        current_frame.graph_instance_id = Some(graph_instance_id);
        current_frame.activity_key = Some("implement".to_string());
        let assignment = AgentAssignment::new(
            run_id,
            graph_instance_id,
            "implement",
            2,
            agent_id,
            launch_frame_id,
        );
        let frame_repo = TestFrameRepo {
            frames: [(current_frame.id, current_frame)].into_iter().collect(),
        };
        let assignment_repo = TestAssignmentRepo {
            assignments: vec![assignment.clone()],
        };
        let run_repo = TestRunRepo {
            runs: [(run_id, run)].into_iter().collect(),
        };
        let mut anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "sess-1",
            run_id,
            launch_frame_id,
            agent_id,
            Some(graph_instance_id),
            Some("implement".to_string()),
        );
        anchor.fill_assignment(assignment.id, assignment.attempt);
        let anchor_repo = TestAnchorRepo {
            anchors: [("sess-1".to_string(), anchor)].into_iter().collect(),
        };
        let resolver =
            ActivityRuntimeAssociationResolver::new(&frame_repo, &assignment_repo, &run_repo);
        let resolver = resolver.with_anchor_repo(&anchor_repo);

        let association = resolver
            .resolve_by_runtime_session("sess-1")
            .await
            .expect("resolver should not error")
            .expect("association should resolve");

        assert_eq!(association.assignment.id, assignment.id);
        assert_eq!(association.assignment.frame_id, launch_frame_id);
        assert_eq!(association.assignment.graph_instance_id, graph_instance_id);
        assert_eq!(association.attempt, 2);
    }

    #[tokio::test]
    async fn resolver_prefers_execution_anchor_assignment_evidence() {
        let project_id = Uuid::new_v4();
        let lifecycle_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let graph_instance_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let launch_frame_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id, lifecycle_id);
        run.id = run_id;
        let assignment = AgentAssignment::new(
            run_id,
            graph_instance_id,
            "custom_main",
            3,
            agent_id,
            launch_frame_id,
        );
        let mut anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "sess-anchor",
            run_id,
            launch_frame_id,
            agent_id,
            Some(graph_instance_id),
            Some("custom_main".to_string()),
        );
        anchor.fill_assignment(assignment.id, assignment.attempt);
        let frame_repo = TestFrameRepo::default();
        let assignment_repo = TestAssignmentRepo {
            assignments: vec![assignment.clone()],
        };
        let run_repo = TestRunRepo {
            runs: [(run_id, run)].into_iter().collect(),
        };
        let anchor_repo = TestAnchorRepo {
            anchors: [("sess-anchor".to_string(), anchor)].into_iter().collect(),
        };
        let resolver =
            ActivityRuntimeAssociationResolver::new(&frame_repo, &assignment_repo, &run_repo)
                .with_anchor_repo(&anchor_repo);

        let association = resolver
            .resolve_by_runtime_session("sess-anchor")
            .await
            .expect("anchor resolver should not error")
            .expect("anchor assignment should resolve");

        assert_eq!(association.assignment.id, assignment.id);
        assert_eq!(association.assignment.activity_key, "custom_main");
        assert_eq!(association.attempt, 3);
    }

    #[tokio::test]
    async fn resolver_errors_when_lifecycle_frame_has_no_assignment() {
        let project_id = Uuid::new_v4();
        let lifecycle_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id, lifecycle_id);
        run.id = run_id;
        let mut current_frame = AgentFrame::new_revision(agent_id, 2, "capability_update");
        current_frame.graph_instance_id = Some(Uuid::new_v4());
        current_frame.activity_key = Some("implement".to_string());
        let frame_id = current_frame.id;
        let graph_instance_id = current_frame.graph_instance_id;
        let activity_key = current_frame.activity_key.clone();
        let frame_repo = TestFrameRepo {
            frames: [(frame_id, current_frame)].into_iter().collect(),
        };
        let assignment_repo = TestAssignmentRepo::default();
        let run_repo = TestRunRepo {
            runs: [(run_id, run)].into_iter().collect(),
        };
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "sess-1",
            run_id,
            frame_id,
            agent_id,
            graph_instance_id,
            activity_key,
        );
        let anchor_repo = TestAnchorRepo {
            anchors: [("sess-1".to_string(), anchor)].into_iter().collect(),
        };
        let resolver =
            ActivityRuntimeAssociationResolver::new(&frame_repo, &assignment_repo, &run_repo);
        let resolver = resolver.with_anchor_repo(&anchor_repo);

        let error = resolver
            .resolve_by_runtime_session("sess-1")
            .await
            .expect_err("missing assignment should be an application error");

        assert!(matches!(
            error,
            ActivityRuntimeAssociationError::MissingAssignment {
                runtime_session_id,
                frame_id: id,
                agent_id: owner,
            } if runtime_session_id == "sess-1" && id == frame_id && owner == agent_id
        ));
    }

    #[tokio::test]
    async fn resolver_ignores_agent_surface_frame_without_activity_scope() {
        let project_id = Uuid::new_v4();
        let lifecycle_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id, lifecycle_id);
        run.id = run_id;
        let current_frame = AgentFrame::new_revision(agent_id, 1, "agent_launch");
        let frame_repo = TestFrameRepo {
            frames: [(current_frame.id, current_frame)].into_iter().collect(),
        };
        let assignment_repo = TestAssignmentRepo::default();
        let run_repo = TestRunRepo {
            runs: [(run_id, run)].into_iter().collect(),
        };
        let resolver =
            ActivityRuntimeAssociationResolver::new(&frame_repo, &assignment_repo, &run_repo);

        assert!(
            resolver
                .resolve_by_runtime_session("sess-1")
                .await
                .expect("surface frame without activity scope should be non-activity runtime")
                .is_none()
        );
    }

    #[tokio::test]
    async fn resolver_returns_none_for_runtime_session_without_frame() {
        let frame_repo = TestFrameRepo::default();
        let assignment_repo = TestAssignmentRepo::default();
        let run_repo = TestRunRepo::default();
        let resolver =
            ActivityRuntimeAssociationResolver::new(&frame_repo, &assignment_repo, &run_repo);

        assert!(
            resolver
                .resolve_by_runtime_session("not-lifecycle")
                .await
                .expect("missing frame should not error")
                .is_none()
        );
    }
}
