use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::project::{
    Project, ProjectRepository, ProjectRole, ProjectSubjectGrant, ProjectSubjectType,
    ProjectVisibility,
};

pub struct PostgresProjectRepository {
    pool: PgPool,
}

impl PostgresProjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["projects", "project_subject_grants"],
        )
        .await
    }
}

#[async_trait::async_trait]
impl ProjectRepository for PostgresProjectRepository {
    async fn create(&self, project: &Project) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(super::db_err)?;

        sqlx::query(
            "INSERT INTO projects (
                id, name, description, config, created_by_user_id, updated_by_user_id,
                visibility, is_template, cloned_from_project_id, created_at, updated_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(project.id.to_string())
        .bind(&project.name)
        .bind(&project.description)
        .bind(serde_json::to_string(&project.config)?)
        .bind(&project.created_by_user_id)
        .bind(&project.updated_by_user_id)
        .bind(project.visibility.as_str())
        .bind(project.is_template)
        .bind(project.cloned_from_project_id.map(|id| id.to_string()))
        .bind(project.created_at)
        .bind(project.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(super::db_err)?;

        let owner_grant = ProjectSubjectGrant::new(
            project.id,
            ProjectSubjectType::User,
            project.created_by_user_id.clone(),
            ProjectRole::Owner,
            project.created_by_user_id.clone(),
        );
        self.upsert_subject_grant_in_tx(&mut tx, &owner_grant)
            .await?;

        tx.commit().await.map_err(super::db_err)?;

        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<Project>, DomainError> {
        let row = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, name, description, config, created_by_user_id, updated_by_user_id,
                    visibility, is_template, cloned_from_project_id, created_at, updated_at
             FROM projects WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;

        row.map(|r| r.try_into()).transpose()
    }

    async fn list_all(&self) -> Result<Vec<Project>, DomainError> {
        let rows = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, name, description, config, created_by_user_id, updated_by_user_id,
                    visibility, is_template, cloned_from_project_id, created_at, updated_at
             FROM projects ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn update(&self, project: &Project) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE projects SET
                name = $1,
                description = $2,
                config = $3,
                created_by_user_id = $4,
                updated_by_user_id = $5,
                visibility = $6,
                is_template = $7,
                cloned_from_project_id = $8,
                updated_at = $9
             WHERE id = $10",
        )
        .bind(&project.name)
        .bind(&project.description)
        .bind(serde_json::to_string(&project.config)?)
        .bind(&project.created_by_user_id)
        .bind(&project.updated_by_user_id)
        .bind(project.visibility.as_str())
        .bind(project.is_template)
        .bind(project.cloned_from_project_id.map(|id| id.to_string()))
        .bind(project.updated_at)
        .bind(project.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "project",
                id: project.id.to_string(),
            });
        }
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(super::db_err)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "project",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    async fn list_subject_grants(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<ProjectSubjectGrant>, DomainError> {
        let rows = sqlx::query_as::<_, ProjectSubjectGrantRow>(
            r#"
            SELECT project_id, subject_type, subject_id, role, granted_by_user_id, created_at, updated_at
            FROM project_subject_grants
            WHERE project_id = $1
            ORDER BY subject_type ASC, subject_id ASC
            "#,
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn upsert_subject_grant(&self, grant: &ProjectSubjectGrant) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(super::db_err)?;

        self.upsert_subject_grant_in_tx(&mut tx, grant).await?;

        tx.commit().await.map_err(super::db_err)?;

        Ok(())
    }

    async fn delete_subject_grant(
        &self,
        project_id: uuid::Uuid,
        subject_type: ProjectSubjectType,
        subject_id: &str,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            DELETE FROM project_subject_grants
            WHERE project_id = $1 AND subject_type = $2 AND subject_id = $3
            "#,
        )
        .bind(project_id.to_string())
        .bind(subject_type.as_str())
        .bind(subject_id)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;

        Ok(())
    }
}

impl PostgresProjectRepository {
    async fn upsert_subject_grant_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        grant: &ProjectSubjectGrant,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO project_subject_grants (
                project_id, subject_type, subject_id, role, granted_by_user_id, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT(project_id, subject_type, subject_id) DO UPDATE SET
                role = excluded.role,
                granted_by_user_id = excluded.granted_by_user_id,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(grant.project_id.to_string())
        .bind(grant.subject_type.as_str())
        .bind(&grant.subject_id)
        .bind(grant.role.as_str())
        .bind(&grant.granted_by_user_id)
        .bind(grant.created_at)
        .bind(chrono::Utc::now())
        .execute(&mut **tx)
        .await
        .map_err(super::db_err)?;

        Ok(())
    }
}

// --- SQLx 行映射辅助结构 ---

#[derive(sqlx::FromRow)]
struct ProjectRow {
    id: String,
    name: String,
    description: String,
    config: String,
    created_by_user_id: String,
    updated_by_user_id: String,
    visibility: String,
    is_template: bool,
    cloned_from_project_id: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct ProjectSubjectGrantRow {
    project_id: String,
    subject_type: String,
    subject_id: String,
    role: String,
    granted_by_user_id: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<ProjectRow> for Project {
    type Error = DomainError;

    fn try_from(row: ProjectRow) -> Result<Self, Self::Error> {
        Ok(Project {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "project",
                id: row.id.clone(),
            })?,
            name: row.name,
            description: row.description,
            config: parse_json_column(&row.config, "projects.config")?,
            created_by_user_id: row.created_by_user_id,
            updated_by_user_id: row.updated_by_user_id,
            visibility: parse_project_visibility(&row.visibility)?,
            is_template: row.is_template,
            cloned_from_project_id: row
                .cloned_from_project_id
                .map(|id| {
                    id.parse().map_err(|error| {
                        DomainError::InvalidConfig(format!(
                            "projects.cloned_from_project_id: {error}"
                        ))
                    })
                })
                .transpose()?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl TryFrom<ProjectSubjectGrantRow> for ProjectSubjectGrant {
    type Error = DomainError;

    fn try_from(row: ProjectSubjectGrantRow) -> Result<Self, Self::Error> {
        Ok(ProjectSubjectGrant {
            project_id: row.project_id.parse().map_err(|_| {
                DomainError::InvalidConfig(String::from("无效的 project grant project_id"))
            })?,
            subject_type: parse_project_subject_type(&row.subject_type)?,
            subject_id: row.subject_id,
            role: parse_project_role(&row.role)?,
            granted_by_user_id: row.granted_by_user_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    field: &str,
) -> Result<T, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn parse_project_visibility(value: &str) -> Result<ProjectVisibility, DomainError> {
    match value {
        "private" => Ok(ProjectVisibility::Private),
        "template_visible" => Ok(ProjectVisibility::TemplateVisible),
        _ => Err(DomainError::InvalidConfig(format!(
            "projects.visibility: 未知值 `{value}`"
        ))),
    }
}

fn parse_project_role(value: &str) -> Result<ProjectRole, DomainError> {
    match value {
        "owner" => Ok(ProjectRole::Owner),
        "editor" => Ok(ProjectRole::Editor),
        "member" => Ok(ProjectRole::Member),
        _ => Err(DomainError::InvalidConfig(format!(
            "project_subject_grants.role: 未知值 `{value}`"
        ))),
    }
}

fn parse_project_subject_type(value: &str) -> Result<ProjectSubjectType, DomainError> {
    match value {
        "user" => Ok(ProjectSubjectType::User),
        "group" => Ok(ProjectSubjectType::Group),
        _ => Err(DomainError::InvalidConfig(format!(
            "project_subject_grants.subject_type: 未知值 `{value}`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::postgres::test_pg_pool;

    async fn new_repo() -> Option<PostgresProjectRepository> {
        let pool = match test_pg_pool("project_repository").await {
            Some(pool) => pool,
            None => return None,
        };
        let repo = PostgresProjectRepository::new(pool);
        repo.initialize().await.expect("应能初始化 project schema");
        Some(repo)
    }

    #[tokio::test]
    async fn create_project_persists_owner_grant_and_audit_fields() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let source_project_id = uuid::Uuid::new_v4();

        let mut project = Project::new_with_creator(
            "Enterprise Auth".to_string(),
            "project grant test".to_string(),
            "alice".to_string(),
        );
        project.visibility = ProjectVisibility::TemplateVisible;
        project.is_template = true;
        project.cloned_from_project_id = Some(source_project_id);

        ProjectRepository::create(&repo, &project)
            .await
            .expect("应能创建 project");

        let persisted = ProjectRepository::get_by_id(&repo, project.id)
            .await
            .expect("应能读取 project")
            .expect("project 应存在");
        let grants = ProjectRepository::list_subject_grants(&repo, project.id)
            .await
            .expect("应能列出 grants");

        assert_eq!(persisted.created_by_user_id, "alice");
        assert_eq!(persisted.updated_by_user_id, "alice");
        assert_eq!(persisted.visibility, ProjectVisibility::TemplateVisible);
        assert!(persisted.is_template);
        assert_eq!(persisted.cloned_from_project_id, Some(source_project_id));
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].subject_type, ProjectSubjectType::User);
        assert_eq!(grants[0].subject_id, "alice");
        assert_eq!(grants[0].role, ProjectRole::Owner);
    }

    #[tokio::test]
    async fn upsert_subject_grant_updates_existing_role() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let project = Project::new_with_creator(
            "Enterprise Auth".to_string(),
            "project grant test".to_string(),
            "owner".to_string(),
        );
        ProjectRepository::create(&repo, &project)
            .await
            .expect("应能创建 project");

        let grant = ProjectSubjectGrant::new(
            project.id,
            ProjectSubjectType::Group,
            "eng".to_string(),
            ProjectRole::Member,
            "owner".to_string(),
        );
        ProjectRepository::upsert_subject_grant(&repo, &grant)
            .await
            .expect("应能插入 group member grant");

        let updated_grant = ProjectSubjectGrant::new(
            project.id,
            ProjectSubjectType::Group,
            "eng".to_string(),
            ProjectRole::Editor,
            "owner".to_string(),
        );
        ProjectRepository::upsert_subject_grant(&repo, &updated_grant)
            .await
            .expect("应能更新 group grant");

        let grants = ProjectRepository::list_subject_grants(&repo, project.id)
            .await
            .expect("应能列出 grants");
        let grant = grants
            .into_iter()
            .find(|entry| {
                entry.subject_type == ProjectSubjectType::Group && entry.subject_id == "eng"
            })
            .expect("应存在 group grant");

        assert_eq!(grant.role, ProjectRole::Editor);
    }
}
