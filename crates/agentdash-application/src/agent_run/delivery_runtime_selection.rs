use agentdash_domain::DomainError;
use agentdash_domain::workflow::{
    AgentFrameRepository, DeliveryBindingStatus, LifecycleAgent, LifecycleAgentRepository,
    LifecycleRunRepository, RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
    SubjectRef,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::ApplicationError;
use crate::lifecycle::{
    AgentRunRuntimeAddress, MessageStreamProjectionRef, MessageStreamTraceKind,
};
use crate::repository_set::RepositorySet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryRuntimeSelectionPolicy {
    CurrentDelivery {
        run_id: Uuid,
        agent_id: Uuid,
    },
    RunScopedLatest {
        run_id: Uuid,
        agent_id: Option<Uuid>,
    },
    LaunchPrimary {
        run_id: Uuid,
        agent_id: Uuid,
    },
    SubjectLatestObserved {
        subject: SubjectRef,
    },
}

#[derive(Debug, Clone)]
pub struct DeliveryRuntimeSelection {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub current_frame_id: Uuid,
    pub launch_frame_id: Uuid,
    pub runtime_session_id: String,
    pub orchestration_id: Option<Uuid>,
    pub node_path: Option<String>,
    pub node_attempt: Option<u32>,
    pub status: DeliveryBindingStatus,
    pub observed_at: DateTime<Utc>,
    pub address: AgentRunRuntimeAddress,
    pub message_stream: MessageStreamProjectionRef,
    pub anchor: RuntimeSessionExecutionAnchor,
}

#[derive(Debug, thiserror::Error)]
pub enum DeliveryRuntimeSelectionError {
    #[error("LifecycleRun {run_id} 不存在")]
    RunNotFound { run_id: Uuid },
    #[error("LifecycleAgent {agent_id} 不存在")]
    AgentNotFound { agent_id: Uuid },
    #[error("LifecycleAgent {agent_id} 属于 run {actual_run_id}，不匹配请求 run {run_id}")]
    AgentRunMismatch {
        run_id: Uuid,
        agent_id: Uuid,
        actual_run_id: Uuid,
    },
    #[error("LifecycleAgent {agent_id} 缺少 current delivery binding")]
    CurrentDeliveryMissing { run_id: Uuid, agent_id: Uuid },
    #[error("LifecycleAgent {agent_id} 的 current delivery binding 缺少字段 {field}")]
    BindingIncomplete {
        run_id: Uuid,
        agent_id: Uuid,
        field: &'static str,
    },
    #[error("RuntimeSessionExecutionAnchor {runtime_session_id} 不存在")]
    AnchorMissing { runtime_session_id: String },
    #[error(
        "RuntimeSessionExecutionAnchor {runtime_session_id} 指向 run {actual_run_id}/agent {actual_agent_id}/launch frame {actual_launch_frame_id}，不匹配期望 run {expected_run_id}/agent {expected_agent_id}/launch frame {expected_launch_frame_id}"
    )]
    AnchorMismatch {
        runtime_session_id: String,
        expected_run_id: Uuid,
        expected_agent_id: Uuid,
        expected_launch_frame_id: Uuid,
        actual_run_id: Uuid,
        actual_agent_id: Uuid,
        actual_launch_frame_id: Uuid,
    },
    #[error("LifecycleAgent {agent_id} 缺少 current_frame_id")]
    CurrentFrameMissing { agent_id: Uuid },
    #[error("AgentFrame {frame_id} 不存在")]
    CurrentFrameNotFound { frame_id: Uuid },
    #[error("Launch AgentFrame {frame_id} 不存在")]
    LaunchFrameNotFound { frame_id: Uuid },
    #[error("Subject {kind}/{id} 不存在")]
    SubjectNotFound { kind: String, id: Uuid },
    #[error("Subject {kind}/{id} 缺少 latest observed delivery history")]
    SubjectDeliveryMissing { kind: String, id: Uuid },
    #[error("run {run_id} 中没有匹配的 runtime delivery anchor")]
    RuntimeDeliveryNotFound {
        run_id: Uuid,
        agent_id: Option<Uuid>,
    },
    #[error(transparent)]
    Repository(#[from] DomainError),
}

impl From<DeliveryRuntimeSelectionError> for ApplicationError {
    fn from(error: DeliveryRuntimeSelectionError) -> Self {
        match error {
            DeliveryRuntimeSelectionError::RunNotFound { .. }
            | DeliveryRuntimeSelectionError::AgentNotFound { .. }
            | DeliveryRuntimeSelectionError::CurrentFrameNotFound { .. }
            | DeliveryRuntimeSelectionError::LaunchFrameNotFound { .. }
            | DeliveryRuntimeSelectionError::SubjectNotFound { .. } => {
                ApplicationError::NotFound(error.to_string())
            }
            DeliveryRuntimeSelectionError::Repository(source) => ApplicationError::from(source),
            other => ApplicationError::Conflict(other.to_string()),
        }
    }
}

pub struct DeliveryRuntimeSelectionRepositories<'a> {
    pub lifecycle_runs: &'a dyn LifecycleRunRepository,
    pub lifecycle_agents: &'a dyn LifecycleAgentRepository,
    pub agent_frames: &'a dyn AgentFrameRepository,
    pub execution_anchors: &'a dyn RuntimeSessionExecutionAnchorRepository,
}

impl<'a> DeliveryRuntimeSelectionRepositories<'a> {
    pub fn from_repository_set(repos: &'a RepositorySet) -> Self {
        Self {
            lifecycle_runs: repos.lifecycle_run_repo.as_ref(),
            lifecycle_agents: repos.lifecycle_agent_repo.as_ref(),
            agent_frames: repos.agent_frame_repo.as_ref(),
            execution_anchors: repos.execution_anchor_repo.as_ref(),
        }
    }
}

pub struct DeliveryRuntimeSelectionService<'a> {
    repos: DeliveryRuntimeSelectionRepositories<'a>,
}

impl<'a> DeliveryRuntimeSelectionService<'a> {
    pub fn new(repos: DeliveryRuntimeSelectionRepositories<'a>) -> Self {
        Self { repos }
    }

    pub fn from_repository_set(repos: &'a RepositorySet) -> Self {
        Self::new(DeliveryRuntimeSelectionRepositories::from_repository_set(
            repos,
        ))
    }

    pub async fn select(
        &self,
        policy: DeliveryRuntimeSelectionPolicy,
    ) -> Result<DeliveryRuntimeSelection, DeliveryRuntimeSelectionError> {
        match policy {
            DeliveryRuntimeSelectionPolicy::CurrentDelivery { run_id, agent_id } => {
                self.select_current_delivery(run_id, agent_id).await
            }
            DeliveryRuntimeSelectionPolicy::RunScopedLatest { run_id, agent_id } => {
                self.select_run_scoped_latest(run_id, agent_id).await
            }
            DeliveryRuntimeSelectionPolicy::LaunchPrimary { run_id, agent_id } => {
                self.select_launch_primary(run_id, agent_id).await
            }
            DeliveryRuntimeSelectionPolicy::SubjectLatestObserved { subject } => {
                Err(DeliveryRuntimeSelectionError::SubjectDeliveryMissing {
                    kind: subject.kind,
                    id: subject.id,
                })
            }
        }
    }

    async fn select_current_delivery(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<DeliveryRuntimeSelection, DeliveryRuntimeSelectionError> {
        self.ensure_run(run_id).await?;
        let agent = self.load_agent_for_run(run_id, agent_id).await?;
        let current_frame_id = agent
            .current_frame_id
            .ok_or(DeliveryRuntimeSelectionError::CurrentFrameMissing { agent_id })?;
        let binding = agent
            .current_delivery
            .clone()
            .ok_or(DeliveryRuntimeSelectionError::CurrentDeliveryMissing { run_id, agent_id })?;
        let anchor = self
            .repos
            .execution_anchors
            .find_by_session(&binding.runtime_session_id)
            .await?
            .ok_or_else(|| DeliveryRuntimeSelectionError::AnchorMissing {
                runtime_session_id: binding.runtime_session_id.clone(),
            })?;
        validate_anchor_matches(&anchor, run_id, agent_id, binding.launch_frame_id)?;

        self.selection_from_anchor(
            agent,
            current_frame_id,
            anchor,
            binding.status,
            binding.observed_at,
        )
        .await
    }

    async fn select_run_scoped_latest(
        &self,
        run_id: Uuid,
        agent_id: Option<Uuid>,
    ) -> Result<DeliveryRuntimeSelection, DeliveryRuntimeSelectionError> {
        self.ensure_run(run_id).await?;
        if let Some(agent_id) = agent_id {
            self.load_agent_for_run(run_id, agent_id).await?;
        }
        let anchor = self
            .repos
            .execution_anchors
            .list_by_run(run_id)
            .await?
            .into_iter()
            .filter(|anchor| agent_id.map_or(true, |id| anchor.agent_id == id))
            .max_by_key(|anchor| anchor.updated_at)
            .ok_or(DeliveryRuntimeSelectionError::RuntimeDeliveryNotFound { run_id, agent_id })?;
        self.selection_from_raw_anchor(anchor).await
    }

    async fn select_launch_primary(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<DeliveryRuntimeSelection, DeliveryRuntimeSelectionError> {
        self.ensure_run(run_id).await?;
        self.load_agent_for_run(run_id, agent_id).await?;
        let anchor = self
            .repos
            .execution_anchors
            .list_by_agent(agent_id)
            .await?
            .into_iter()
            .filter(|anchor| anchor.run_id == run_id)
            .min_by_key(|anchor| anchor.created_at)
            .ok_or(DeliveryRuntimeSelectionError::RuntimeDeliveryNotFound {
                run_id,
                agent_id: Some(agent_id),
            })?;
        self.selection_from_raw_anchor(anchor).await
    }

    async fn ensure_run(&self, run_id: Uuid) -> Result<(), DeliveryRuntimeSelectionError> {
        self.repos
            .lifecycle_runs
            .get_by_id(run_id)
            .await?
            .ok_or(DeliveryRuntimeSelectionError::RunNotFound { run_id })?;
        Ok(())
    }

    async fn load_agent_for_run(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<LifecycleAgent, DeliveryRuntimeSelectionError> {
        let agent = self
            .repos
            .lifecycle_agents
            .get(agent_id)
            .await?
            .ok_or(DeliveryRuntimeSelectionError::AgentNotFound { agent_id })?;
        if agent.run_id != run_id {
            return Err(DeliveryRuntimeSelectionError::AgentRunMismatch {
                run_id,
                agent_id,
                actual_run_id: agent.run_id,
            });
        }
        Ok(agent)
    }

    async fn selection_from_raw_anchor(
        &self,
        anchor: RuntimeSessionExecutionAnchor,
    ) -> Result<DeliveryRuntimeSelection, DeliveryRuntimeSelectionError> {
        let agent = self
            .load_agent_for_run(anchor.run_id, anchor.agent_id)
            .await?;
        let current_frame_id =
            agent
                .current_frame_id
                .ok_or(DeliveryRuntimeSelectionError::CurrentFrameMissing {
                    agent_id: anchor.agent_id,
                })?;
        let (status, observed_at) = agent
            .current_delivery
            .as_ref()
            .filter(|binding| binding.runtime_session_id == anchor.runtime_session_id)
            .map(|binding| (binding.status, binding.observed_at))
            .unwrap_or((DeliveryBindingStatus::Ready, anchor.updated_at));
        self.selection_from_anchor(agent, current_frame_id, anchor, status, observed_at)
            .await
    }

    async fn selection_from_anchor(
        &self,
        agent: LifecycleAgent,
        current_frame_id: Uuid,
        anchor: RuntimeSessionExecutionAnchor,
        status: DeliveryBindingStatus,
        observed_at: DateTime<Utc>,
    ) -> Result<DeliveryRuntimeSelection, DeliveryRuntimeSelectionError> {
        self.repos.agent_frames.get(current_frame_id).await?.ok_or(
            DeliveryRuntimeSelectionError::CurrentFrameNotFound {
                frame_id: current_frame_id,
            },
        )?;
        self.repos
            .agent_frames
            .get(anchor.launch_frame_id)
            .await?
            .ok_or(DeliveryRuntimeSelectionError::LaunchFrameNotFound {
                frame_id: anchor.launch_frame_id,
            })?;

        Ok(DeliveryRuntimeSelection {
            run_id: anchor.run_id,
            agent_id: anchor.agent_id,
            current_frame_id,
            launch_frame_id: anchor.launch_frame_id,
            runtime_session_id: anchor.runtime_session_id.clone(),
            orchestration_id: anchor.orchestration_id,
            node_path: anchor.node_path.clone(),
            node_attempt: anchor.node_attempt,
            status,
            observed_at,
            address: AgentRunRuntimeAddress {
                run_id: anchor.run_id,
                agent_id: agent.id,
                frame_id: current_frame_id,
            },
            message_stream: MessageStreamProjectionRef {
                runtime_session_id: anchor.runtime_session_id.clone(),
                trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
            },
            anchor,
        })
    }
}

fn validate_anchor_matches(
    anchor: &RuntimeSessionExecutionAnchor,
    expected_run_id: Uuid,
    expected_agent_id: Uuid,
    expected_launch_frame_id: Uuid,
) -> Result<(), DeliveryRuntimeSelectionError> {
    if anchor.run_id == expected_run_id
        && anchor.agent_id == expected_agent_id
        && anchor.launch_frame_id == expected_launch_frame_id
    {
        return Ok(());
    }
    Err(DeliveryRuntimeSelectionError::AnchorMismatch {
        runtime_session_id: anchor.runtime_session_id.clone(),
        expected_run_id,
        expected_agent_id,
        expected_launch_frame_id,
        actual_run_id: anchor.run_id,
        actual_agent_id: anchor.agent_id,
        actual_launch_frame_id: anchor.launch_frame_id,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_domain::workflow::{
        AgentFrame, AgentFrameRepository, AgentSource, LifecycleAgent, LifecycleAgentRepository,
        LifecycleRun, LifecycleRunRepository, RuntimeSessionExecutionAnchor,
        RuntimeSessionExecutionAnchorRepository, SubjectRef,
    };
    use tokio::sync::Mutex;

    use super::*;
    use crate::test_support::{
        MemoryAgentFrameRepository, MemoryLifecycleAgentRepository,
        MemoryRuntimeSessionExecutionAnchorRepository,
    };

    #[derive(Default)]
    struct MemoryLifecycleRunRepository {
        runs: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait::async_trait]
    impl LifecycleRunRepository for MemoryLifecycleRunRepository {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().await.push(run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .filter(|run| ids.contains(&run.id))
                .cloned()
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut runs = self.runs.lock().await;
            if let Some(existing) = runs.iter_mut().find(|item| item.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().await.retain(|run| run.id != id);
            Ok(())
        }
    }

    struct SelectionFixture {
        runs: Arc<MemoryLifecycleRunRepository>,
        agents: Arc<MemoryLifecycleAgentRepository>,
        frames: Arc<MemoryAgentFrameRepository>,
        anchors: Arc<MemoryRuntimeSessionExecutionAnchorRepository>,
    }

    impl SelectionFixture {
        fn new() -> Self {
            Self {
                runs: Arc::new(MemoryLifecycleRunRepository::default()),
                agents: Arc::new(MemoryLifecycleAgentRepository::default()),
                frames: Arc::new(MemoryAgentFrameRepository::default()),
                anchors: Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default()),
            }
        }

        fn service(&self) -> DeliveryRuntimeSelectionService<'_> {
            DeliveryRuntimeSelectionService::new(DeliveryRuntimeSelectionRepositories {
                lifecycle_runs: self.runs.as_ref(),
                lifecycle_agents: self.agents.as_ref(),
                agent_frames: self.frames.as_ref(),
                execution_anchors: self.anchors.as_ref(),
            })
        }

        async fn seed_current_delivery(
            &self,
        ) -> (
            LifecycleRun,
            LifecycleAgent,
            AgentFrame,
            AgentFrame,
            RuntimeSessionExecutionAnchor,
        ) {
            let run = LifecycleRun::new_plain(Uuid::new_v4());
            self.runs.create(&run).await.expect("create run");

            let mut agent =
                LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
            let launch_frame = AgentFrame::new_initial(agent.id);
            let current_frame = AgentFrame::new_revision(agent.id, 2, "test");
            agent.set_current_frame(current_frame.id);
            let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
                "runtime-current",
                run.id,
                launch_frame.id,
                agent.id,
            );
            agent.bind_current_delivery_from_anchor(
                &anchor,
                DeliveryBindingStatus::Running,
                anchor.updated_at,
            );

            self.frames
                .create(&launch_frame)
                .await
                .expect("launch frame");
            self.frames
                .create(&current_frame)
                .await
                .expect("current frame");
            self.anchors.upsert(&anchor).await.expect("anchor");
            self.agents.create(&agent).await.expect("agent");

            (run, agent, launch_frame, current_frame, anchor)
        }
    }

    #[tokio::test]
    async fn delivery_runtime_selection_current_delivery_returns_binding_coordinate() {
        let fixture = SelectionFixture::new();
        let (run, agent, launch_frame, current_frame, anchor) =
            fixture.seed_current_delivery().await;

        let selection = fixture
            .service()
            .select(DeliveryRuntimeSelectionPolicy::CurrentDelivery {
                run_id: run.id,
                agent_id: agent.id,
            })
            .await
            .expect("selection");

        assert_eq!(selection.run_id, run.id);
        assert_eq!(selection.agent_id, agent.id);
        assert_eq!(selection.current_frame_id, current_frame.id);
        assert_eq!(selection.launch_frame_id, launch_frame.id);
        assert_eq!(selection.runtime_session_id, "runtime-current");
        assert_eq!(selection.status, DeliveryBindingStatus::Running);
        assert_eq!(selection.address.frame_id, current_frame.id);
        assert_eq!(
            selection.message_stream.runtime_session_id,
            anchor.runtime_session_id
        );
        assert_eq!(selection.anchor.launch_frame_id, launch_frame.id);
    }

    #[tokio::test]
    async fn delivery_runtime_selection_current_delivery_requires_binding() {
        let fixture = SelectionFixture::new();
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let mut agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        let frame = AgentFrame::new_initial(agent.id);
        agent.set_current_frame(frame.id);
        fixture.runs.create(&run).await.expect("run");
        fixture.frames.create(&frame).await.expect("frame");
        fixture.agents.create(&agent).await.expect("agent");

        let error = fixture
            .service()
            .select(DeliveryRuntimeSelectionPolicy::CurrentDelivery {
                run_id: run.id,
                agent_id: agent.id,
            })
            .await
            .expect_err("missing binding");

        assert!(matches!(
            error,
            DeliveryRuntimeSelectionError::CurrentDeliveryMissing { .. }
        ));
    }

    #[tokio::test]
    async fn delivery_runtime_selection_current_delivery_rejects_anchor_mismatch() {
        let fixture = SelectionFixture::new();
        let (run, mut agent, launch_frame, current_frame, anchor) =
            fixture.seed_current_delivery().await;
        let mismatched_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            anchor.runtime_session_id.clone(),
            Uuid::new_v4(),
            launch_frame.id,
            agent.id,
        );
        fixture
            .anchors
            .upsert(&mismatched_anchor)
            .await
            .expect("replace anchor");
        agent.set_current_frame(current_frame.id);

        let error = fixture
            .service()
            .select(DeliveryRuntimeSelectionPolicy::CurrentDelivery {
                run_id: run.id,
                agent_id: agent.id,
            })
            .await
            .expect_err("mismatch");

        assert!(matches!(
            error,
            DeliveryRuntimeSelectionError::AnchorMismatch { .. }
        ));
    }

    #[tokio::test]
    async fn delivery_runtime_selection_run_scoped_latest_uses_raw_anchor_order_explicitly() {
        let fixture = SelectionFixture::new();
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let mut agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        let launch_frame = AgentFrame::new_initial(agent.id);
        let current_frame = AgentFrame::new_revision(agent.id, 2, "test");
        agent.set_current_frame(current_frame.id);
        fixture.runs.create(&run).await.expect("run");
        fixture
            .frames
            .create(&launch_frame)
            .await
            .expect("launch frame");
        fixture
            .frames
            .create(&current_frame)
            .await
            .expect("current frame");
        fixture.agents.create(&agent).await.expect("agent");

        let mut older = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-old",
            run.id,
            launch_frame.id,
            agent.id,
        );
        older.updated_at = Utc::now() - chrono::Duration::seconds(5);
        let newer = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-new",
            run.id,
            launch_frame.id,
            agent.id,
        );
        fixture.anchors.upsert(&older).await.expect("older");
        fixture.anchors.upsert(&newer).await.expect("newer");

        let selection = fixture
            .service()
            .select(DeliveryRuntimeSelectionPolicy::RunScopedLatest {
                run_id: run.id,
                agent_id: Some(agent.id),
            })
            .await
            .expect("selection");

        assert_eq!(selection.runtime_session_id, "runtime-new");
        assert_eq!(selection.status, DeliveryBindingStatus::Ready);
    }

    #[tokio::test]
    async fn delivery_runtime_selection_launch_primary_uses_earliest_anchor() {
        let fixture = SelectionFixture::new();
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let mut agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        let launch_frame = AgentFrame::new_initial(agent.id);
        let current_frame = AgentFrame::new_revision(agent.id, 2, "test");
        agent.set_current_frame(current_frame.id);
        fixture.runs.create(&run).await.expect("run");
        fixture
            .frames
            .create(&launch_frame)
            .await
            .expect("launch frame");
        fixture
            .frames
            .create(&current_frame)
            .await
            .expect("current frame");
        fixture.agents.create(&agent).await.expect("agent");

        let mut primary = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-primary",
            run.id,
            launch_frame.id,
            agent.id,
        );
        primary.created_at = Utc::now() - chrono::Duration::seconds(5);
        let secondary = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-secondary",
            run.id,
            launch_frame.id,
            agent.id,
        );
        fixture.anchors.upsert(&secondary).await.expect("secondary");
        fixture.anchors.upsert(&primary).await.expect("primary");

        let selection = fixture
            .service()
            .select(DeliveryRuntimeSelectionPolicy::LaunchPrimary {
                run_id: run.id,
                agent_id: agent.id,
            })
            .await
            .expect("selection");

        assert_eq!(selection.runtime_session_id, "runtime-primary");
    }

    #[tokio::test]
    async fn delivery_runtime_selection_subject_latest_observed_is_boundary_only() {
        let fixture = SelectionFixture::new();
        let subject = SubjectRef::new("story", Uuid::new_v4());

        let error = fixture
            .service()
            .select(DeliveryRuntimeSelectionPolicy::SubjectLatestObserved { subject })
            .await
            .expect_err("subject history not implemented");

        assert!(matches!(
            error,
            DeliveryRuntimeSelectionError::SubjectDeliveryMissing { .. }
        ));
    }
}
