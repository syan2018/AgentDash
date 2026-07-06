use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::backend::{
    ProjectBackendAccess, ProjectBackendAccessRepository, ProjectBackendAccessStatus,
};

use crate::ApplicationError;

pub const PROJECT_BACKEND_ACCESS_NOTE_RUNNER_REGISTRATION_TOKEN: &str = "runner_registration_token";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectBackendAccessGrantSource {
    UserGrant,
    RunnerRegistrationToken,
}

impl ProjectBackendAccessGrantSource {
    fn default_note(self) -> Option<&'static str> {
        match self {
            Self::UserGrant => None,
            Self::RunnerRegistrationToken => {
                Some(PROJECT_BACKEND_ACCESS_NOTE_RUNNER_REGISTRATION_TOKEN)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnsureProjectBackendAccessGrantInput {
    pub project_id: Uuid,
    pub backend_id: String,
    pub source: ProjectBackendAccessGrantSource,
    pub created_by_user_id: Option<String>,
    pub priority: Option<i32>,
    pub root_policy: Option<serde_json::Value>,
    pub capability_policy: Option<serde_json::Value>,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EnsureProjectBackendAccessGrantResult {
    pub access: ProjectBackendAccess,
    pub created: bool,
    pub reactivated: bool,
}

pub async fn ensure_project_backend_access_grant(
    project_backend_access_repo: &dyn ProjectBackendAccessRepository,
    input: EnsureProjectBackendAccessGrantInput,
) -> Result<EnsureProjectBackendAccessGrantResult, ApplicationError> {
    let backend_id = normalize_required("backend_id", &input.backend_id)?;
    let requested_note = normalize_optional_string(input.note)
        .or_else(|| input.source.default_note().map(str::to_string));

    if let Some(existing) = project_backend_access_repo
        .list_by_project(input.project_id)
        .await?
        .into_iter()
        .find(|access| access.backend_id == backend_id)
    {
        let was_active = existing.status == ProjectBackendAccessStatus::Active;
        let mut access = existing;
        access.status = ProjectBackendAccessStatus::Active;
        if let Some(priority) = input.priority {
            access.priority = priority;
        }
        if let Some(root_policy) = input.root_policy {
            access.root_policy = root_policy;
        }
        if let Some(capability_policy) = input.capability_policy {
            access.capability_policy = capability_policy;
        }
        if let Some(note) = requested_note {
            access.note = Some(note);
        }
        project_backend_access_repo.update(&access).await?;
        let stored = project_backend_access_repo
            .get_by_id(access.id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Internal("ProjectBackendAccess 更新后读取失败".to_string())
            })?;
        return Ok(EnsureProjectBackendAccessGrantResult {
            access: stored,
            created: false,
            reactivated: !was_active,
        });
    }

    let mut access = ProjectBackendAccess::new(
        input.project_id,
        backend_id.clone(),
        input.created_by_user_id.clone(),
    );
    access.priority = input.priority.unwrap_or_default();
    if let Some(root_policy) = input.root_policy {
        access.root_policy = root_policy;
    }
    if let Some(capability_policy) = input.capability_policy {
        access.capability_policy = capability_policy;
    }
    access.note = requested_note;

    match project_backend_access_repo.create(&access).await {
        Ok(()) => Ok(EnsureProjectBackendAccessGrantResult {
            access,
            created: true,
            reactivated: false,
        }),
        Err(DomainError::Conflict { .. }) => {
            if let Some(active) = project_backend_access_repo
                .get_active_for_project_backend(input.project_id, &backend_id)
                .await?
            {
                Ok(EnsureProjectBackendAccessGrantResult {
                    access: active,
                    created: false,
                    reactivated: false,
                })
            } else {
                Err(ApplicationError::Conflict(
                    "ProjectBackendAccess 并发创建冲突".to_string(),
                ))
            }
        }
        Err(error) => Err(ApplicationError::from(error)),
    }
}

fn normalize_required(field: &str, raw: &str) -> Result<String, ApplicationError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ApplicationError::BadRequest(format!("{field} 不能为空")));
    }
    Ok(value.to_string())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FixtureProjectBackendAccessRepository {
        accesses: Mutex<Vec<ProjectBackendAccess>>,
        create_result: Mutex<CreateResult>,
    }

    #[derive(Default)]
    enum CreateResult {
        #[default]
        Ok,
        ConflictAfterInsert,
        ConflictWithoutInsert,
    }

    impl FixtureProjectBackendAccessRepository {
        async fn push(&self, access: ProjectBackendAccess) {
            self.accesses.lock().await.push(access);
        }

        async fn set_create_result(&self, result: CreateResult) {
            *self.create_result.lock().await = result;
        }

        async fn accesses(&self) -> Vec<ProjectBackendAccess> {
            self.accesses.lock().await.clone()
        }
    }

    #[async_trait::async_trait]
    impl ProjectBackendAccessRepository for FixtureProjectBackendAccessRepository {
        async fn create(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
            match &*self.create_result.lock().await {
                CreateResult::Ok => {
                    self.push(access.clone()).await;
                    Ok(())
                }
                CreateResult::ConflictAfterInsert => {
                    self.push(access.clone()).await;
                    Err(conflict_error())
                }
                CreateResult::ConflictWithoutInsert => Err(conflict_error()),
            }
        }

        async fn update(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
            let mut accesses = self.accesses.lock().await;
            if let Some(existing) = accesses.iter_mut().find(|item| item.id == access.id) {
                *existing = access.clone();
            }
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .iter()
                .find(|access| access.id == id)
                .cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .iter()
                .filter(|access| access.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_active_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .list_by_project(project_id)
                .await?
                .into_iter()
                .filter(ProjectBackendAccess::is_active)
                .collect())
        }

        async fn get_active_for_project_backend(
            &self,
            project_id: Uuid,
            backend_id: &str,
        ) -> Result<Option<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .iter()
                .find(|access| {
                    access.project_id == project_id
                        && access.backend_id == backend_id
                        && access.is_active()
                })
                .cloned())
        }

        async fn list_active_by_backend(
            &self,
            backend_id: &str,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .iter()
                .filter(|access| access.backend_id == backend_id && access.is_active())
                .cloned()
                .collect())
        }

        async fn list_active_by_backends(
            &self,
            backend_ids: &[String],
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .iter()
                .filter(|access| {
                    backend_ids.iter().any(|id| id == &access.backend_id) && access.is_active()
                })
                .cloned()
                .collect())
        }

        async fn set_status(
            &self,
            id: Uuid,
            status: ProjectBackendAccessStatus,
        ) -> Result<(), DomainError> {
            if let Some(access) = self
                .accesses
                .lock()
                .await
                .iter_mut()
                .find(|item| item.id == id)
            {
                access.status = status;
            }
            Ok(())
        }
    }

    fn input(
        project_id: Uuid,
        backend_id: &str,
        source: ProjectBackendAccessGrantSource,
    ) -> EnsureProjectBackendAccessGrantInput {
        EnsureProjectBackendAccessGrantInput {
            project_id,
            backend_id: backend_id.to_string(),
            source,
            created_by_user_id: Some("user-a".to_string()),
            priority: None,
            root_policy: None,
            capability_policy: None,
            note: None,
        }
    }

    #[tokio::test]
    async fn ensure_grant_creates_access_with_source_defaults() {
        let repo = FixtureProjectBackendAccessRepository::default();
        let project_id = Uuid::new_v4();

        let result = ensure_project_backend_access_grant(
            &repo,
            input(
                project_id,
                "backend-a",
                ProjectBackendAccessGrantSource::RunnerRegistrationToken,
            ),
        )
        .await
        .expect("ensure grant");

        assert!(result.created);
        assert_eq!(repo.accesses().await.len(), 1);
        assert_eq!(result.access.project_id, project_id);
        assert_eq!(result.access.backend_id, "backend-a");
        assert_eq!(result.access.status, ProjectBackendAccessStatus::Active);
        assert_eq!(
            result.access.note.as_deref(),
            Some(PROJECT_BACKEND_ACCESS_NOTE_RUNNER_REGISTRATION_TOKEN)
        );
    }

    #[tokio::test]
    async fn ensure_grant_reactivates_existing_access_and_applies_options() {
        let repo = FixtureProjectBackendAccessRepository::default();
        let project_id = Uuid::new_v4();
        let mut existing =
            ProjectBackendAccess::new(project_id, "backend-a".to_string(), Some("old".into()));
        existing.status = ProjectBackendAccessStatus::Revoked;
        repo.push(existing).await;

        let mut request = input(
            project_id,
            "backend-a",
            ProjectBackendAccessGrantSource::UserGrant,
        );
        request.priority = Some(7);
        request.note = Some("  manual note  ".to_string());
        let result = ensure_project_backend_access_grant(&repo, request)
            .await
            .expect("ensure grant");

        assert!(!result.created);
        assert!(result.reactivated);
        assert_eq!(result.access.priority, 7);
        assert_eq!(result.access.note.as_deref(), Some("manual note"));
        assert_eq!(result.access.status, ProjectBackendAccessStatus::Active);
    }

    #[tokio::test]
    async fn ensure_grant_treats_conflict_with_active_row_as_idempotent() {
        let repo = Arc::new(FixtureProjectBackendAccessRepository::default());
        repo.set_create_result(CreateResult::ConflictAfterInsert)
            .await;
        let project_id = Uuid::new_v4();

        let result = ensure_project_backend_access_grant(
            repo.as_ref(),
            input(
                project_id,
                "backend-a",
                ProjectBackendAccessGrantSource::UserGrant,
            ),
        )
        .await
        .expect("conflict after insert should resolve");

        assert!(!result.created);
        assert_eq!(result.access.backend_id, "backend-a");
    }

    #[tokio::test]
    async fn ensure_grant_surfaces_conflict_without_active_row() {
        let repo = FixtureProjectBackendAccessRepository::default();
        repo.set_create_result(CreateResult::ConflictWithoutInsert)
            .await;

        let error = ensure_project_backend_access_grant(
            &repo,
            input(
                Uuid::new_v4(),
                "backend-a",
                ProjectBackendAccessGrantSource::UserGrant,
            ),
        )
        .await
        .expect_err("conflict without row should surface");

        assert!(matches!(error, ApplicationError::Conflict(_)));
    }

    fn conflict_error() -> DomainError {
        DomainError::Conflict {
            entity: "project_backend_access",
            constraint: "test",
            message: "conflict".to_string(),
        }
    }
}
