use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::project::{
    Project, ProjectConfig, ProjectRepository, ProjectRole, ProjectSubjectGrant,
    ProjectSubjectType, ProjectVisibility,
};

pub struct SqliteProjectRepository {
    pool: PgPool,
}

impl SqliteProjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                config TEXT NOT NULL DEFAULT '{}',
                created_by_user_id TEXT NOT NULL DEFAULT 'system',
                updated_by_user_id TEXT NOT NULL DEFAULT 'system',
                visibility TEXT NOT NULL DEFAULT 'private',
                is_template INTEGER NOT NULL DEFAULT 0,
                cloned_from_project_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS project_subject_grants (
                project_id TEXT NOT NULL,
                subject_type TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                role TEXT NOT NULL,
                granted_by_user_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (project_id, subject_type, subject_id),
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_project_subject_grants_subject ON project_subject_grants(subject_type, subject_id)"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl ProjectRepository for SqliteProjectRepository {
    async fn create(&self, project: &Project) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
        .bind(project.created_at.to_rfc3339())
        .bind(project.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let owner_grant = ProjectSubjectGrant::new(
            project.id,
            ProjectSubjectType::User,
            project.created_by_user_id.clone(),
            ProjectRole::Owner,
            project.created_by_user_id.clone(),
        );
        self.upsert_subject_grant_in_tx(&mut tx, &owner_grant)
            .await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
        .bind(project.updated_at.to_rfc3339())
        .bind(project.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn upsert_subject_grant(&self, grant: &ProjectSubjectGrant) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.upsert_subject_grant_in_tx(&mut tx, grant).await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

impl SqliteProjectRepository {
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
        .bind(grant.created_at.to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
    created_at: String,
    updated_at: String,
}

#[derive(sqlx::FromRow)]
struct ProjectSubjectGrantRow {
    project_id: String,
    subject_type: String,
    subject_id: String,
    role: String,
    granted_by_user_id: String,
    created_at: String,
    updated_at: String,
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
            config: serde_json::from_str::<ProjectConfig>(&row.config).unwrap_or_default(),
            created_by_user_id: row.created_by_user_id,
            updated_by_user_id: row.updated_by_user_id,
            visibility: parse_project_visibility(&row.visibility),
            is_template: row.is_template,
            cloned_from_project_id: row.cloned_from_project_id.and_then(|id| id.parse().ok()),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}

impl TryFrom<ProjectSubjectGrantRow> for ProjectSubjectGrant {
    type Error = DomainError;

    fn try_from(row: ProjectSubjectGrantRow) -> Result<Self, Self::Error> {
        Ok(ProjectSubjectGrant {
            project_id: row.project_id.parse().map_err(|_| {
                DomainError::InvalidConfig("无效的 project grant project_id".to_string())
            })?,
            subject_type: parse_project_subject_type(&row.subject_type),
            subject_id: row.subject_id,
            role: parse_project_role(&row.role),
            granted_by_user_id: row.granted_by_user_id,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}

fn parse_project_visibility(value: &str) -> ProjectVisibility {
    match value {
        "template_visible" => ProjectVisibility::TemplateVisible,
        _ => ProjectVisibility::Private,
    }
}

fn parse_project_role(value: &str) -> ProjectRole {
    match value {
        "editor" => ProjectRole::Editor,
        "viewer" => ProjectRole::Viewer,
        _ => ProjectRole::Owner,
    }
}

fn parse_project_subject_type(value: &str) -> ProjectSubjectType {
    match value {
        "group" => ProjectSubjectType::Group,
        _ => ProjectSubjectType::User,
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;

    async fn new_repo() -> SqliteProjectRepository {
        let database_url =
            std::env::var("TEST_DATABASE_URL").expect("运行测试前需设置 TEST_DATABASE_URL");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("应能连接测试 PostgreSQL");
        let repo = SqliteProjectRepository::new(pool);
        repo.initialize().await.expect("应能初始化 project schema");
        repo
    }

    #[tokio::test]
    async fn create_project_persists_owner_grant_and_audit_fields() {
        let repo = new_repo().await;
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
        let repo = new_repo().await;
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
            ProjectRole::Viewer,
            "owner".to_string(),
        );
        ProjectRepository::upsert_subject_grant(&repo, &grant)
            .await
            .expect("应能插入 group viewer grant");

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
