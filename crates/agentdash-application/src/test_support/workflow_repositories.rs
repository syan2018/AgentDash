use agentdash_domain::DomainError;
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, LifecycleAgent, LifecycleAgentRepository, LifecycleGate,
    LifecycleGateRepository, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository,
};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct MemoryAgentFrameRepository {
    frames: Mutex<Vec<AgentFrame>>,
}

impl MemoryAgentFrameRepository {
    pub(crate) async fn first_frame(&self) -> Option<AgentFrame> {
        self.frames.lock().await.iter().next().cloned()
    }
}

#[async_trait::async_trait]
impl AgentFrameRepository for MemoryAgentFrameRepository {
    async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
        self.frames.lock().await.push(frame.clone());
        Ok(())
    }

    async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        Ok(self
            .frames
            .lock()
            .await
            .iter()
            .find(|frame| frame.id == frame_id)
            .cloned())
    }

    async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        Ok(self
            .frames
            .lock()
            .await
            .iter()
            .filter(|frame| frame.agent_id == agent_id)
            .max_by_key(|frame| frame.revision)
            .cloned())
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
        Ok(self
            .frames
            .lock()
            .await
            .iter()
            .filter(|frame| frame.agent_id == agent_id)
            .cloned()
            .collect())
    }

    async fn append_visible_canvas_mount(
        &self,
        frame_id: Uuid,
        mount_id: &str,
    ) -> Result<(), DomainError> {
        let mut frames = self.frames.lock().await;
        let frame = frames
            .iter_mut()
            .find(|frame| frame.id == frame_id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_frame",
                id: frame_id.to_string(),
            })?;
        frame.append_visible_canvas_mount(mount_id);
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct MemoryRuntimeSessionExecutionAnchorRepository {
    anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
}

#[async_trait::async_trait]
impl RuntimeSessionExecutionAnchorRepository for MemoryRuntimeSessionExecutionAnchorRepository {
    async fn upsert(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
        let mut anchors = self.anchors.lock().await;
        if let Some(existing) = anchors
            .iter_mut()
            .find(|item| item.runtime_session_id == anchor.runtime_session_id)
        {
            *existing = anchor.clone();
        } else {
            anchors.push(anchor.clone());
        }
        Ok(())
    }

    async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
        self.anchors
            .lock()
            .await
            .retain(|anchor| anchor.runtime_session_id != runtime_session_id);
        Ok(())
    }

    async fn find_by_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(self
            .anchors
            .lock()
            .await
            .iter()
            .find(|anchor| anchor.runtime_session_id == runtime_session_id)
            .cloned())
    }

    async fn list_by_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(self
            .anchors
            .lock()
            .await
            .iter()
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
            .await
            .iter()
            .filter(|anchor| anchor.agent_id == agent_id)
            .cloned()
            .collect())
    }

    async fn list_by_project_session_ids(
        &self,
        runtime_session_ids: &[String],
    ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(self
            .anchors
            .lock()
            .await
            .iter()
            .filter(|anchor| runtime_session_ids.contains(&anchor.runtime_session_id))
            .cloned()
            .collect())
    }

    async fn latest_for_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(self
            .anchors
            .lock()
            .await
            .iter()
            .filter(|anchor| anchor.agent_id == agent_id)
            .max_by_key(|anchor| anchor.created_at)
            .cloned())
    }
}

#[derive(Default)]
pub(crate) struct MemoryLifecycleGateRepository {
    gates: Mutex<Vec<LifecycleGate>>,
}

#[async_trait::async_trait]
impl LifecycleGateRepository for MemoryLifecycleGateRepository {
    async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        self.gates.lock().await.push(gate.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .await
            .iter()
            .find(|gate| gate.id == id)
            .cloned())
    }

    async fn list_open_for_agent(&self, agent_id: Uuid) -> Result<Vec<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .await
            .iter()
            .filter(|gate| gate.agent_id == Some(agent_id) && gate.is_open())
            .cloned()
            .collect())
    }

    async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        let mut gates = self.gates.lock().await;
        let existing = gates
            .iter_mut()
            .find(|existing| existing.id == gate.id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "lifecycle_gate",
                id: gate.id.to_string(),
            })?;
        *existing = gate.clone();
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct MemoryLifecycleAgentRepository {
    agents: Mutex<Vec<LifecycleAgent>>,
}

#[async_trait::async_trait]
impl LifecycleAgentRepository for MemoryLifecycleAgentRepository {
    async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
        self.agents.lock().await.push(agent.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
        Ok(self
            .agents
            .lock()
            .await
            .iter()
            .find(|agent| agent.id == id)
            .cloned())
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
        Ok(self
            .agents
            .lock()
            .await
            .iter()
            .filter(|agent| agent.run_id == run_id)
            .cloned()
            .collect())
    }

    async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
        let mut agents = self.agents.lock().await;
        if let Some(existing) = agents.iter_mut().find(|item| item.id == agent.id) {
            *existing = agent.clone();
        }
        Ok(())
    }
}
