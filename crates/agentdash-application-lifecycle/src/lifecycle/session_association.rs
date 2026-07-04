use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, LifecycleAgent, LifecycleAgentRepository, LifecycleRun,
    LifecycleRunRepository, RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
};
use uuid::Uuid;

/// Lifecycle node 子 session 的 binding label 前缀。
pub const LIFECYCLE_NODE_LABEL_PREFIX: &str = "lifecycle_node:";
pub const LIFECYCLE_ACTIVITY_LABEL_PREFIX: &str = "lifecycle_activity:";

/// 子 session 与 lifecycle runtime node 的关联解析结果。
#[derive(Debug, Clone)]
pub struct LifecycleActivitySessionAssociation {
    pub run: LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

/// RuntimeSession terminal / advance 事件解析出的稳定 lifecycle execution 证据。
///
/// anchor 表达 runtime session 启动时的 launch frame evidence。
/// resolver 必须允许 runtime session 的当前 frame revision 演进后仍能回到 runtime node。
#[derive(Debug, Clone)]
pub struct ActivityRuntimeAssociation {
    pub run: LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

pub type RuntimeSessionCurrentFrame = (RuntimeSessionExecutionAnchor, LifecycleAgent, AgentFrame);

/// 从 delivery trace ref 回溯当前 AgentFrame surface。
///
/// RuntimeSession 只作为 trace/delivery evidence；业务 owner 来自 anchor 反查出的
/// LifecycleAgent 与当前 AgentFrame。
pub async fn resolve_current_frame_from_delivery_trace_ref(
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
    let Some(frame) = frame_repo.get_current(agent.id).await? else {
        return Ok(None);
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
        "runtime session {runtime_session_id} 对应 AgentFrame {frame_id} 与 anchor agent 不匹配: agent_id={agent_id}"
    )]
    AnchorFrameMismatch {
        runtime_session_id: String,
        frame_id: Uuid,
        agent_id: Uuid,
    },
    #[error("runtime session {runtime_session_id} 指向的 lifecycle run 不存在: {run_id}")]
    MissingLifecycleRunForAnchor {
        runtime_session_id: String,
        run_id: Uuid,
    },
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
    run_repo: &'a dyn LifecycleRunRepository,
    anchor_repo: Option<&'a dyn RuntimeSessionExecutionAnchorRepository>,
}

impl<'a> ActivityRuntimeAssociationResolver<'a> {
    pub fn new(
        frame_repo: &'a dyn AgentFrameRepository,
        run_repo: &'a dyn LifecycleRunRepository,
    ) -> Self {
        Self {
            frame_repo,
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

    pub async fn resolve_by_message_stream_trace(
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
        let Some(orchestration_id) = anchor.orchestration_id else {
            return Ok(None);
        };
        let Some(node_path) = anchor.node_path.clone() else {
            return Ok(None);
        };
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
        if frame.agent_id != anchor.agent_id {
            return Err(ActivityRuntimeAssociationError::AnchorFrameMismatch {
                runtime_session_id: session_id.to_string(),
                frame_id: anchor.launch_frame_id,
                agent_id: anchor.agent_id,
            });
        }

        let attempt = anchor.node_attempt.unwrap_or(1);
        let run = self
            .run_repo
            .get_by_id(anchor.run_id)
            .await
            .map_err(|e| ActivityRuntimeAssociationError::Repository {
                operation: "lifecycle run",
                message: e.to_string(),
            })?
            .ok_or(
                ActivityRuntimeAssociationError::MissingLifecycleRunForAnchor {
                    runtime_session_id: session_id.to_string(),
                    run_id: anchor.run_id,
                },
            )?;
        Ok(Some(ActivityRuntimeAssociation {
            run,
            orchestration_id,
            node_path,
            attempt,
        }))
    }
}

/// 从 message stream trace 解析 lifecycle runtime node 执行证据。
pub async fn resolve_activity_runtime_association_from_message_stream_trace(
    session_id: &str,
    frame_repo: &dyn AgentFrameRepository,
    _agent_repo: &dyn LifecycleAgentRepository,
    run_repo: &dyn LifecycleRunRepository,
    anchor_repo: Option<&dyn RuntimeSessionExecutionAnchorRepository>,
) -> Result<Option<LifecycleActivitySessionAssociation>, ActivityRuntimeAssociationError> {
    let mut resolver = ActivityRuntimeAssociationResolver::new(frame_repo, run_repo);
    if let Some(anchor_repo) = anchor_repo {
        resolver = resolver.with_anchor_repo(anchor_repo);
    }
    Ok(resolver
        .resolve_by_message_stream_trace(session_id)
        .await?
        .map(|association| {
            let ActivityRuntimeAssociation {
                run,
                orchestration_id,
                node_path,
                attempt,
                ..
            } = association;
            LifecycleActivitySessionAssociation {
                run,
                orchestration_id,
                node_path,
                attempt,
            }
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::build_lifecycle_mount_with_node_scope;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        AgentFrame, AgentFrameRepository, AgentSource, LifecycleAgent, LifecycleAgentRepository,
        LifecycleRunRepository,
    };
    use agentdash_spi::Vfs;
    use std::collections::HashMap;
    use std::sync::Mutex;

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
    }

    #[derive(Default)]
    struct TestAgentRepo {
        agents: HashMap<Uuid, LifecycleAgent>,
    }

    #[async_trait::async_trait]
    impl LifecycleAgentRepository for TestAgentRepo {
        async fn create(&self, _agent: &LifecycleAgent) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(self.agents.get(&id).cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .values()
                .filter(|agent| agent.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, _agent: &LifecycleAgent) -> Result<(), DomainError> {
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

        async fn update(&self, _run: &LifecycleRun) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct TestAnchorRepo {
        anchors: Mutex<HashMap<String, RuntimeSessionExecutionAnchor>>,
    }

    impl TestAnchorRepo {
        fn seeded(
            anchors: impl IntoIterator<Item = (String, RuntimeSessionExecutionAnchor)>,
        ) -> Self {
            Self {
                anchors: Mutex::new(anchors.into_iter().collect()),
            }
        }
    }

    #[async_trait::async_trait]
    impl RuntimeSessionExecutionAnchorRepository for TestAnchorRepo {
        async fn create_once(
            &self,
            anchor: &RuntimeSessionExecutionAnchor,
        ) -> Result<(), DomainError> {
            let mut anchors = self.anchors.lock().unwrap();
            if let Some(existing) = anchors.get(&anchor.runtime_session_id) {
                if existing.has_same_launch_coordinates_as(anchor) {
                    return Ok(());
                }
                return Err(existing.immutable_conflict(anchor));
            }
            anchors.insert(anchor.runtime_session_id.clone(), anchor.clone());
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.anchors.lock().unwrap().remove(runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .get(runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
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
                .lock()
                .unwrap()
                .values()
                .filter(|anchor| anchor.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            let anchors = self.anchors.lock().unwrap();
            Ok(runtime_session_ids
                .iter()
                .filter_map(|id| anchors.get(id).cloned())
                .collect())
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

    #[tokio::test]
    async fn resolver_uses_orchestration_anchor_after_runtime_frame_revision_changes() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let orchestration_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let launch_frame_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id);
        run.id = run_id;
        let mut launch_frame = AgentFrame::new_revision(agent_id, 1, "launch");
        launch_frame.id = launch_frame_id;
        let frame_repo = TestFrameRepo {
            frames: [(launch_frame_id, launch_frame)].into_iter().collect(),
        };
        let run_repo = TestRunRepo {
            runs: [(run_id, run)].into_iter().collect(),
        };
        let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            "sess-1",
            run_id,
            launch_frame_id,
            agent_id,
            orchestration_id,
            "implement",
            2,
        );
        let anchor_repo = TestAnchorRepo::seeded([("sess-1".to_string(), anchor)]);
        let resolver = ActivityRuntimeAssociationResolver::new(&frame_repo, &run_repo);
        let resolver = resolver.with_anchor_repo(&anchor_repo);

        let association = resolver
            .resolve_by_message_stream_trace("sess-1")
            .await
            .expect("resolver should not error")
            .expect("association should resolve");

        assert_eq!(association.orchestration_id, orchestration_id);
        assert_eq!(association.node_path, "implement");
        assert_eq!(association.attempt, 2);
    }

    #[tokio::test]
    async fn resolver_uses_execution_anchor_node_evidence() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let orchestration_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let launch_frame_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id);
        run.id = run_id;
        let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            "sess-anchor",
            run_id,
            launch_frame_id,
            agent_id,
            orchestration_id,
            "custom_main",
            3,
        );
        let mut frame = AgentFrame::new_revision(agent_id, 1, "test");
        frame.id = launch_frame_id;
        let frame_repo = TestFrameRepo {
            frames: [(launch_frame_id, frame)].into_iter().collect(),
        };
        let run_repo = TestRunRepo {
            runs: [(run_id, run)].into_iter().collect(),
        };
        let anchor_repo = TestAnchorRepo::seeded([("sess-anchor".to_string(), anchor)]);
        let resolver = ActivityRuntimeAssociationResolver::new(&frame_repo, &run_repo)
            .with_anchor_repo(&anchor_repo);

        let association = resolver
            .resolve_by_message_stream_trace("sess-anchor")
            .await
            .expect("anchor resolver should not error")
            .expect("anchor runtime node should resolve");

        assert_eq!(association.orchestration_id, orchestration_id);
        assert_eq!(association.node_path, "custom_main");
        assert_eq!(association.attempt, 3);
    }

    #[tokio::test]
    async fn runtime_session_current_frame_exposes_lifecycle_vfs_surface() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let orchestration_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::WorkflowAgent);

        let launch_frame = AgentFrame::new_revision(agent.id, 1, "launch");
        let lifecycle_mount = build_lifecycle_mount_with_node_scope(
            run_id,
            orchestration_id,
            "agent",
            "test_lifecycle",
            &["result".to_string()],
            Some(1),
        );
        let lifecycle_vfs = Vfs {
            mounts: vec![lifecycle_mount],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let mut current_frame = AgentFrame::new_revision(agent.id, 2, "lifecycle_surface");
        current_frame.vfs_surface_json = serde_json::to_value(&lifecycle_vfs).ok();

        let frame_repo = TestFrameRepo {
            frames: [
                (launch_frame.id, launch_frame.clone()),
                (current_frame.id, current_frame.clone()),
            ]
            .into_iter()
            .collect(),
        };
        let agent_repo = TestAgentRepo {
            agents: [(agent.id, agent)].into_iter().collect(),
        };
        let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            "sess-vfs",
            run_id,
            launch_frame.id,
            current_frame.agent_id,
            orchestration_id,
            "agent",
            1,
        );
        let anchor_repo = TestAnchorRepo::seeded([("sess-vfs".to_string(), anchor)]);

        let (_anchor, _agent, frame) = resolve_current_frame_from_delivery_trace_ref(
            "sess-vfs",
            &anchor_repo,
            &agent_repo,
            &frame_repo,
        )
        .await
        .expect("current frame lookup should not error")
        .expect("runtime session should resolve to current frame");

        assert_eq!(frame.id, current_frame.id);
        let vfs: Vfs = serde_json::from_value(
            frame
                .vfs_surface_json
                .clone()
                .expect("current frame should expose VFS"),
        )
        .expect("current frame should expose typed VFS");
        assert!(
            vfs.mounts
                .iter()
                .any(|mount| { mount.id == "lifecycle" && mount.provider == "lifecycle_vfs" })
        );
    }

    #[tokio::test]
    async fn resolver_returns_none_for_plain_anchor() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id);
        run.id = run_id;
        let current_frame = AgentFrame::new_revision(agent_id, 2, "capability_update");
        let frame_id = current_frame.id;
        let frame_repo = TestFrameRepo {
            frames: [(frame_id, current_frame)].into_iter().collect(),
        };
        let run_repo = TestRunRepo {
            runs: [(run_id, run)].into_iter().collect(),
        };
        let anchor =
            RuntimeSessionExecutionAnchor::new_dispatch("sess-1", run_id, frame_id, agent_id);
        let anchor_repo = TestAnchorRepo::seeded([("sess-1".to_string(), anchor)]);
        let resolver = ActivityRuntimeAssociationResolver::new(&frame_repo, &run_repo);
        let resolver = resolver.with_anchor_repo(&anchor_repo);

        let association = resolver
            .resolve_by_message_stream_trace("sess-1")
            .await
            .expect("plain anchor should not error");
        assert!(association.is_none());
    }

    #[tokio::test]
    async fn resolver_returns_none_without_anchor_repo() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id);
        run.id = run_id;
        let current_frame = AgentFrame::new_revision(agent_id, 1, "agent_launch");
        let frame_repo = TestFrameRepo {
            frames: [(current_frame.id, current_frame)].into_iter().collect(),
        };
        let run_repo = TestRunRepo {
            runs: [(run_id, run)].into_iter().collect(),
        };
        let resolver = ActivityRuntimeAssociationResolver::new(&frame_repo, &run_repo);

        assert!(
            resolver
                .resolve_by_message_stream_trace("sess-1")
                .await
                .expect("surface frame without activity scope should be non-activity runtime")
                .is_none()
        );
    }

    #[tokio::test]
    async fn resolver_returns_none_for_runtime_session_without_frame() {
        let frame_repo = TestFrameRepo::default();
        let run_repo = TestRunRepo::default();
        let resolver = ActivityRuntimeAssociationResolver::new(&frame_repo, &run_repo);

        assert!(
            resolver
                .resolve_by_message_stream_trace("not-lifecycle")
                .await
                .expect("missing frame should not error")
                .is_none()
        );
    }
}
