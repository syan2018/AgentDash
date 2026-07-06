use agentdash_domain::DomainError;
use agentdash_domain::canvas::{
    Canvas, CanvasInteractionSnapshot, CanvasRepository, CanvasRuntimeObservation,
    CanvasRuntimeStateRepository,
};
use agentdash_domain::project::{
    Project, ProjectRepository, ProjectSubjectGrant, ProjectSubjectType,
};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct MemoryCanvasRepository {
    canvases: Mutex<Vec<Canvas>>,
}

#[async_trait::async_trait]
impl CanvasRepository for MemoryCanvasRepository {
    async fn create(&self, canvas: &Canvas) -> Result<(), DomainError> {
        self.canvases.lock().await.push(canvas.clone());
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<Canvas>, DomainError> {
        Ok(self
            .canvases
            .lock()
            .await
            .iter()
            .find(|canvas| canvas.id == id)
            .cloned())
    }

    async fn get_by_mount_id(
        &self,
        project_id: Uuid,
        mount_id: &str,
    ) -> Result<Option<Canvas>, DomainError> {
        Ok(self
            .canvases
            .lock()
            .await
            .iter()
            .find(|canvas| canvas.project_id == project_id && canvas.mount_id == mount_id)
            .cloned())
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Canvas>, DomainError> {
        Ok(self
            .canvases
            .lock()
            .await
            .iter()
            .filter(|canvas| canvas.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn find_published_from(
        &self,
        source_canvas_id: Uuid,
    ) -> Result<Option<Canvas>, DomainError> {
        Ok(self
            .canvases
            .lock()
            .await
            .iter()
            .find(|canvas| canvas.published_from_canvas_id == Some(source_canvas_id))
            .cloned())
    }

    async fn update(&self, canvas: &Canvas) -> Result<(), DomainError> {
        let mut canvases = self.canvases.lock().await;
        if let Some(existing) = canvases
            .iter_mut()
            .find(|existing| existing.id == canvas.id)
        {
            *existing = canvas.clone();
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.canvases.lock().await.retain(|canvas| canvas.id != id);
        Ok(())
    }
}

impl MemoryCanvasRepository {
    pub fn new_with_canvases(canvases: Vec<Canvas>) -> Self {
        Self {
            canvases: Mutex::new(canvases),
        }
    }

    pub async fn debug_list(&self) -> Vec<Canvas> {
        self.canvases.lock().await.clone()
    }
}

#[derive(Default)]
pub struct MemoryCanvasRuntimeStateRepository {
    observations: Mutex<Vec<CanvasRuntimeObservation>>,
    snapshots: Mutex<Vec<CanvasInteractionSnapshot>>,
}

#[async_trait::async_trait]
impl CanvasRuntimeStateRepository for MemoryCanvasRuntimeStateRepository {
    async fn upsert_runtime_observation(
        &self,
        observation: CanvasRuntimeObservation,
    ) -> Result<CanvasRuntimeObservation, DomainError> {
        self.observations.lock().await.push(observation.clone());
        Ok(observation)
    }

    async fn latest_runtime_observation(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        canvas_mount_id: &str,
    ) -> Result<Option<CanvasRuntimeObservation>, DomainError> {
        Ok(self
            .observations
            .lock()
            .await
            .iter()
            .filter(|observation| {
                observation.run_id == run_id
                    && observation.agent_id == agent_id
                    && observation.canvas_mount_id == canvas_mount_id
            })
            .max_by_key(|observation| observation.captured_at)
            .cloned())
    }

    async fn upsert_interaction_snapshot(
        &self,
        snapshot: CanvasInteractionSnapshot,
    ) -> Result<CanvasInteractionSnapshot, DomainError> {
        let mut snapshots = self.snapshots.lock().await;
        if let Some(existing) = snapshots.iter_mut().find(|existing| {
            existing.run_id == snapshot.run_id
                && existing.agent_id == snapshot.agent_id
                && existing.canvas_mount_id == snapshot.canvas_mount_id
        }) {
            *existing = snapshot.clone();
        } else {
            snapshots.push(snapshot.clone());
        }
        Ok(snapshot)
    }

    async fn latest_interaction_snapshot(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        canvas_mount_id: &str,
    ) -> Result<Option<CanvasInteractionSnapshot>, DomainError> {
        Ok(self
            .snapshots
            .lock()
            .await
            .iter()
            .filter(|snapshot| {
                snapshot.run_id == run_id
                    && snapshot.agent_id == agent_id
                    && snapshot.canvas_mount_id == canvas_mount_id
            })
            .max_by_key(|snapshot| snapshot.updated_at)
            .cloned())
    }
}

#[derive(Default)]
pub struct MemoryProjectRepository {
    projects: Mutex<Vec<Project>>,
    grants: Mutex<Vec<ProjectSubjectGrant>>,
}

#[async_trait::async_trait]
impl ProjectRepository for MemoryProjectRepository {
    async fn create(&self, project: &Project) -> Result<(), DomainError> {
        self.projects.lock().await.push(project.clone());
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<Project>, DomainError> {
        Ok(self
            .projects
            .lock()
            .await
            .iter()
            .find(|project| project.id == id)
            .cloned())
    }

    async fn list_all(&self) -> Result<Vec<Project>, DomainError> {
        Ok(self.projects.lock().await.clone())
    }

    async fn update(&self, project: &Project) -> Result<(), DomainError> {
        let mut projects = self.projects.lock().await;
        if let Some(existing) = projects
            .iter_mut()
            .find(|existing| existing.id == project.id)
        {
            *existing = project.clone();
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.projects
            .lock()
            .await
            .retain(|project| project.id != id);
        Ok(())
    }

    async fn list_subject_grants(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectSubjectGrant>, DomainError> {
        Ok(self
            .grants
            .lock()
            .await
            .iter()
            .filter(|grant| grant.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn upsert_subject_grant(&self, grant: &ProjectSubjectGrant) -> Result<(), DomainError> {
        let mut grants = self.grants.lock().await;
        if let Some(existing) = grants.iter_mut().find(|existing| {
            existing.project_id == grant.project_id
                && existing.subject_type == grant.subject_type
                && existing.subject_id == grant.subject_id
        }) {
            *existing = grant.clone();
        } else {
            grants.push(grant.clone());
        }
        Ok(())
    }

    async fn delete_subject_grant(
        &self,
        project_id: Uuid,
        subject_type: ProjectSubjectType,
        subject_id: &str,
    ) -> Result<(), DomainError> {
        self.grants.lock().await.retain(|grant| {
            !(grant.project_id == project_id
                && grant.subject_type == subject_type
                && grant.subject_id == subject_id)
        });
        Ok(())
    }
}
