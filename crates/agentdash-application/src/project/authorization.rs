use std::collections::HashSet;

use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::project::{
    Project, ProjectRepository, ProjectRole, ProjectSubjectGrant, ProjectSubjectType,
    ProjectVisibility,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAuthorizationContext {
    pub user_id: String,
    pub group_ids: Vec<String>,
    pub is_admin: bool,
}

impl ProjectAuthorizationContext {
    pub fn new(user_id: String, group_ids: Vec<String>, is_admin: bool) -> Self {
        Self {
            user_id,
            group_ids,
            is_admin,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAuthorization {
    pub role: Option<ProjectRole>,
    pub via_admin_bypass: bool,
    pub via_template_visibility: bool,
}

impl ProjectAuthorization {
    pub fn can_view_project(&self) -> bool {
        self.via_admin_bypass || self.via_template_visibility || self.role.is_some()
    }

    pub fn can_edit_project(&self) -> bool {
        self.via_admin_bypass || matches!(self.role, Some(ProjectRole::Owner | ProjectRole::Editor))
    }

    pub fn can_manage_project_sharing(&self) -> bool {
        self.via_admin_bypass || matches!(self.role, Some(ProjectRole::Owner))
    }

    pub fn can_admin_bypass(&self) -> bool {
        self.via_admin_bypass
    }
}

pub struct ProjectAuthorizationService<'a, R: ?Sized> {
    project_repo: &'a R,
}

impl<'a, R: ?Sized> ProjectAuthorizationService<'a, R>
where
    R: ProjectRepository,
{
    pub fn new(project_repo: &'a R) -> Self {
        Self { project_repo }
    }

    pub async fn resolve_project_access(
        &self,
        context: &ProjectAuthorizationContext,
        project: &Project,
    ) -> Result<ProjectAuthorization, DomainError> {
        if context.is_admin {
            return Ok(ProjectAuthorization {
                role: Some(ProjectRole::Owner),
                via_admin_bypass: true,
                via_template_visibility: false,
            });
        }

        let grants = self.project_repo.list_subject_grants(project.id).await?;
        let role = highest_role_for_subject(context, &grants);
        let via_template_visibility =
            matches!(project.visibility, ProjectVisibility::TemplateVisible);

        Ok(ProjectAuthorization {
            role,
            via_admin_bypass: false,
            via_template_visibility,
        })
    }

    pub async fn list_accessible_projects(
        &self,
        context: &ProjectAuthorizationContext,
    ) -> Result<Vec<Project>, DomainError> {
        let projects = self.project_repo.list_all().await?;
        let mut visible = Vec::new();
        for project in projects {
            if self
                .resolve_project_access(context, &project)
                .await?
                .can_view_project()
            {
                visible.push(project);
            }
        }
        Ok(visible)
    }

    pub async fn would_leave_project_without_owner(
        &self,
        project_id: Uuid,
        subject_type: ProjectSubjectType,
        subject_id: &str,
        next_role: Option<ProjectRole>,
    ) -> Result<bool, DomainError> {
        let grants = self.project_repo.list_subject_grants(project_id).await?;
        let mut owners = grants
            .iter()
            .filter(|grant| grant.role == ProjectRole::Owner)
            .map(|grant| (grant.subject_type, grant.subject_id.clone()))
            .collect::<HashSet<_>>();

        let subject_key = (subject_type, subject_id.to_string());
        owners.remove(&subject_key);

        if matches!(next_role, Some(ProjectRole::Owner)) {
            owners.insert(subject_key);
        }

        Ok(owners.is_empty())
    }
}

fn highest_role_for_subject(
    context: &ProjectAuthorizationContext,
    grants: &[ProjectSubjectGrant],
) -> Option<ProjectRole> {
    let group_ids = context.group_ids.iter().cloned().collect::<HashSet<_>>();

    grants
        .iter()
        .filter(|grant| match grant.subject_type {
            ProjectSubjectType::User => grant.subject_id == context.user_id,
            ProjectSubjectType::Group => group_ids.contains(&grant.subject_id),
        })
        .map(|grant| grant.role)
        .max_by_key(role_rank)
}

fn role_rank(role: &ProjectRole) -> u8 {
    match role {
        ProjectRole::Viewer => 1,
        ProjectRole::Editor => 2,
        ProjectRole::Owner => 3,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use agentdash_domain::project::{Project, ProjectConfig, ProjectVisibility};

    use super::*;

    #[derive(Default)]
    struct MemoryProjectStore {
        projects: Mutex<HashMap<Uuid, Project>>,
        grants: Mutex<HashMap<(Uuid, ProjectSubjectType, String), ProjectSubjectGrant>>,
    }

    #[async_trait]
    impl ProjectRepository for MemoryProjectStore {
        async fn create(&self, project: &Project) -> Result<(), DomainError> {
            self.projects
                .lock()
                .expect("lock")
                .insert(project.id, project.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<Project>, DomainError> {
            Ok(self.projects.lock().expect("lock").get(&id).cloned())
        }

        async fn list_all(&self) -> Result<Vec<Project>, DomainError> {
            Ok(self
                .projects
                .lock()
                .expect("lock")
                .values()
                .cloned()
                .collect())
        }

        async fn update(&self, project: &Project) -> Result<(), DomainError> {
            self.projects
                .lock()
                .expect("lock")
                .insert(project.id, project.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.projects.lock().expect("lock").remove(&id);
            Ok(())
        }

        async fn list_subject_grants(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectSubjectGrant>, DomainError> {
            Ok(self
                .grants
                .lock()
                .expect("lock")
                .values()
                .filter(|grant| grant.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn upsert_subject_grant(
            &self,
            grant: &ProjectSubjectGrant,
        ) -> Result<(), DomainError> {
            self.grants.lock().expect("lock").insert(
                (
                    grant.project_id,
                    grant.subject_type,
                    grant.subject_id.clone(),
                ),
                grant.clone(),
            );
            Ok(())
        }

        async fn delete_subject_grant(
            &self,
            project_id: Uuid,
            subject_type: ProjectSubjectType,
            subject_id: &str,
        ) -> Result<(), DomainError> {
            self.grants.lock().expect("lock").remove(&(
                project_id,
                subject_type,
                subject_id.to_string(),
            ));
            Ok(())
        }
    }

    fn project(visibility: ProjectVisibility) -> Project {
        let mut project = Project::new_with_creator(
            "Auth Test".to_string(),
            "desc".to_string(),
            "owner-user".to_string(),
        );
        project.visibility = visibility;
        project.is_template = matches!(visibility, ProjectVisibility::TemplateVisible);
        project.config = ProjectConfig::default();
        project
    }

    fn context(user_id: &str, group_ids: &[&str], is_admin: bool) -> ProjectAuthorizationContext {
        ProjectAuthorizationContext::new(
            user_id.to_string(),
            group_ids.iter().map(|value| (*value).to_string()).collect(),
            is_admin,
        )
    }

    #[tokio::test]
    async fn owner_grant_can_manage_project_sharing() {
        let store = MemoryProjectStore::default();
        let project = project(ProjectVisibility::Private);
        ProjectRepository::create(&store, &project)
            .await
            .expect("create project");
        ProjectRepository::upsert_subject_grant(
            &store,
            &ProjectSubjectGrant::new(
                project.id,
                ProjectSubjectType::User,
                "owner-user".to_string(),
                ProjectRole::Owner,
                "owner-user".to_string(),
            ),
        )
        .await
        .expect("grant owner");

        let service = ProjectAuthorizationService::new(&store);
        let access = service
            .resolve_project_access(&context("owner-user", &[], false), &project)
            .await
            .expect("resolve access");

        assert!(access.can_view_project());
        assert!(access.can_edit_project());
        assert!(access.can_manage_project_sharing());
    }

    #[tokio::test]
    async fn group_editor_grant_can_edit_but_cannot_manage_sharing() {
        let store = MemoryProjectStore::default();
        let project = project(ProjectVisibility::Private);
        ProjectRepository::create(&store, &project)
            .await
            .expect("create project");
        ProjectRepository::upsert_subject_grant(
            &store,
            &ProjectSubjectGrant::new(
                project.id,
                ProjectSubjectType::Group,
                "eng".to_string(),
                ProjectRole::Editor,
                "owner-user".to_string(),
            ),
        )
        .await
        .expect("grant editor");

        let service = ProjectAuthorizationService::new(&store);
        let access = service
            .resolve_project_access(&context("dev-user", &["eng"], false), &project)
            .await
            .expect("resolve access");

        assert!(access.can_view_project());
        assert!(access.can_edit_project());
        assert!(!access.can_manage_project_sharing());
    }

    #[tokio::test]
    async fn template_visible_project_allows_view_without_grant() {
        let store = MemoryProjectStore::default();
        let project = project(ProjectVisibility::TemplateVisible);
        ProjectRepository::create(&store, &project)
            .await
            .expect("create project");

        let service = ProjectAuthorizationService::new(&store);
        let access = service
            .resolve_project_access(&context("viewer", &[], false), &project)
            .await
            .expect("resolve access");

        assert!(access.can_view_project());
        assert!(!access.can_edit_project());
        assert!(!access.can_manage_project_sharing());
    }

    #[tokio::test]
    async fn admin_bypass_grants_full_access() {
        let store = MemoryProjectStore::default();
        let project = project(ProjectVisibility::Private);
        ProjectRepository::create(&store, &project)
            .await
            .expect("create project");

        let service = ProjectAuthorizationService::new(&store);
        let access = service
            .resolve_project_access(&context("admin", &[], true), &project)
            .await
            .expect("resolve access");

        assert!(access.can_view_project());
        assert!(access.can_edit_project());
        assert!(access.can_manage_project_sharing());
        assert!(access.can_admin_bypass());
    }

    #[tokio::test]
    async fn detects_last_owner_removal() {
        let store = MemoryProjectStore::default();
        let project = project(ProjectVisibility::Private);
        ProjectRepository::create(&store, &project)
            .await
            .expect("create project");
        ProjectRepository::upsert_subject_grant(
            &store,
            &ProjectSubjectGrant::new(
                project.id,
                ProjectSubjectType::User,
                "owner-user".to_string(),
                ProjectRole::Owner,
                "owner-user".to_string(),
            ),
        )
        .await
        .expect("grant owner");

        let service = ProjectAuthorizationService::new(&store);
        let would_break = service
            .would_leave_project_without_owner(
                project.id,
                ProjectSubjectType::User,
                "owner-user",
                None,
            )
            .await
            .expect("evaluate owner removal");

        assert!(would_break);
    }
}
