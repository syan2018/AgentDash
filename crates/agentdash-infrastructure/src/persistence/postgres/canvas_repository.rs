use std::collections::BTreeMap;

use sqlx::{PgPool, Postgres, QueryBuilder};

use agentdash_domain::DomainError;
use agentdash_domain::canvas::{
    Canvas, CanvasDataBinding, CanvasFile, CanvasRepository, CanvasSandboxConfig, CanvasScope,
};

pub struct PostgresCanvasRepository {
    pool: PgPool,
}

impl PostgresCanvasRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["canvases", "canvas_files", "canvas_bindings"],
        )
        .await
    }

    async fn load_files(
        &self,
        canvas_ids: &[String],
    ) -> Result<BTreeMap<String, Vec<CanvasFile>>, DomainError> {
        let mut file_map = BTreeMap::<String, Vec<CanvasFile>>::new();
        for canvas_id in canvas_ids {
            file_map.insert(canvas_id.clone(), Vec::new());
        }
        if canvas_ids.is_empty() {
            return Ok(file_map);
        }

        let rows = sqlx::query_as::<_, CanvasFileWithOwnerRow>(
            r#"
            SELECT canvas_id, path, content
            FROM canvas_files
            WHERE canvas_id = ANY($1)
            ORDER BY canvas_id ASC, path ASC
            "#,
        )
        .bind(canvas_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;

        for row in rows {
            file_map
                .entry(row.canvas_id)
                .or_default()
                .push(CanvasFile::new(row.path, row.content));
        }
        Ok(file_map)
    }

    async fn load_bindings(
        &self,
        canvas_ids: &[String],
    ) -> Result<BTreeMap<String, Vec<CanvasDataBinding>>, DomainError> {
        let mut binding_map = BTreeMap::<String, Vec<CanvasDataBinding>>::new();
        for canvas_id in canvas_ids {
            binding_map.insert(canvas_id.clone(), Vec::new());
        }
        if canvas_ids.is_empty() {
            return Ok(binding_map);
        }

        let rows = sqlx::query_as::<_, CanvasBindingWithOwnerRow>(
            r#"
            SELECT canvas_id, alias, source_uri, content_type
            FROM canvas_bindings
            WHERE canvas_id = ANY($1)
            ORDER BY canvas_id ASC, alias ASC
            "#,
        )
        .bind(canvas_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;

        for row in rows {
            binding_map
                .entry(row.canvas_id)
                .or_default()
                .push(CanvasDataBinding {
                    alias: row.alias,
                    source_uri: row.source_uri,
                    content_type: row.content_type,
                });
        }
        Ok(binding_map)
    }

    async fn replace_files(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        canvas: &Canvas,
    ) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM canvas_files WHERE canvas_id = $1")
            .bind(canvas.id.to_string())
            .execute(&mut **tx)
            .await
            .map_err(super::db_err)?;

        if canvas.files.is_empty() {
            return Ok(());
        }
        let canvas_id = canvas.id.to_string();
        let mut builder: QueryBuilder<Postgres> =
            QueryBuilder::new("INSERT INTO canvas_files (canvas_id, path, content) ");
        builder.push_values(&canvas.files, |mut row, file| {
            row.push_bind(&canvas_id)
                .push_bind(&file.path)
                .push_bind(&file.content);
        });
        builder
            .build()
            .execute(&mut **tx)
            .await
            .map_err(super::db_err)?;

        Ok(())
    }

    async fn replace_bindings(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        canvas: &Canvas,
    ) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM canvas_bindings WHERE canvas_id = $1")
            .bind(canvas.id.to_string())
            .execute(&mut **tx)
            .await
            .map_err(super::db_err)?;

        if canvas.bindings.is_empty() {
            return Ok(());
        }
        let canvas_id = canvas.id.to_string();
        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO canvas_bindings (canvas_id, alias, source_uri, content_type) ",
        );
        builder.push_values(&canvas.bindings, |mut row, binding| {
            row.push_bind(&canvas_id)
                .push_bind(&binding.alias)
                .push_bind(&binding.source_uri)
                .push_bind(&binding.content_type);
        });
        builder
            .build()
            .execute(&mut **tx)
            .await
            .map_err(super::db_err)?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl CanvasRepository for PostgresCanvasRepository {
    async fn create(&self, canvas: &Canvas) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(super::db_err)?;
        let published_from_canvas_id = canvas
            .published_from_canvas_id
            .as_ref()
            .map(ToString::to_string);
        let shared_canvas_id = canvas.shared_canvas_id.as_ref().map(ToString::to_string);
        let cloned_from_canvas_id = canvas
            .cloned_from_canvas_id
            .as_ref()
            .map(ToString::to_string);

        sqlx::query(
            r#"
            INSERT INTO canvases (
                id, project_id, owner_user_id, scope, mount_id, title, description, entry_file,
                sandbox_config, published_from_canvas_id, shared_canvas_id, cloned_from_canvas_id,
                published_at, published_by_user_id, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            "#,
        )
        .bind(canvas.id.to_string())
        .bind(canvas.project_id.to_string())
        .bind(canvas.owner_user_id.as_deref())
        .bind(canvas.scope.as_str())
        .bind(&canvas.mount_id)
        .bind(&canvas.title)
        .bind(&canvas.description)
        .bind(&canvas.entry_file)
        .bind(serde_json::to_string(&canvas.sandbox_config)?)
        .bind(published_from_canvas_id.as_deref())
        .bind(shared_canvas_id.as_deref())
        .bind(cloned_from_canvas_id.as_deref())
        .bind(canvas.published_at)
        .bind(canvas.published_by_user_id.as_deref())
        .bind(canvas.created_at)
        .bind(canvas.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(super::db_err)?;

        self.replace_files(&mut tx, canvas).await?;
        self.replace_bindings(&mut tx, canvas).await?;

        tx.commit().await.map_err(super::db_err)?;

        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<Canvas>, DomainError> {
        let row = sqlx::query_as::<_, CanvasRow>(
            r#"
            SELECT id, project_id, owner_user_id, scope, mount_id, title, description, entry_file,
                   sandbox_config, published_from_canvas_id, shared_canvas_id, cloned_from_canvas_id,
                   published_at, published_by_user_id, created_at, updated_at
            FROM canvases
            WHERE id = $1
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let canvas_id = row.id.clone();
        let files = self.load_files(std::slice::from_ref(&canvas_id)).await?;
        let bindings = self.load_bindings(std::slice::from_ref(&canvas_id)).await?;
        Ok(Some(row.try_into_canvas(files, bindings)?))
    }

    async fn get_by_mount_id(
        &self,
        project_id: uuid::Uuid,
        mount_id: &str,
    ) -> Result<Option<Canvas>, DomainError> {
        let row = sqlx::query_as::<_, CanvasRow>(
            r#"
            SELECT id, project_id, owner_user_id, scope, mount_id, title, description, entry_file,
                   sandbox_config, published_from_canvas_id, shared_canvas_id, cloned_from_canvas_id,
                   published_at, published_by_user_id, created_at, updated_at
            FROM canvases
            WHERE project_id = $1 AND mount_id = $2
            "#,
        )
        .bind(project_id.to_string())
        .bind(mount_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let canvas_id = row.id.clone();
        let files = self.load_files(std::slice::from_ref(&canvas_id)).await?;
        let bindings = self.load_bindings(std::slice::from_ref(&canvas_id)).await?;
        Ok(Some(row.try_into_canvas(files, bindings)?))
    }

    async fn list_by_project(&self, project_id: uuid::Uuid) -> Result<Vec<Canvas>, DomainError> {
        let rows = sqlx::query_as::<_, CanvasRow>(
            r#"
            SELECT id, project_id, owner_user_id, scope, mount_id, title, description, entry_file,
                   sandbox_config, published_from_canvas_id, shared_canvas_id, cloned_from_canvas_id,
                   published_at, published_by_user_id, created_at, updated_at
            FROM canvases
            WHERE project_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;

        let canvas_ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let files = self.load_files(&canvas_ids).await?;
        let bindings = self.load_bindings(&canvas_ids).await?;

        rows.into_iter()
            .map(|row| row.try_into_canvas(files.clone(), bindings.clone()))
            .collect()
    }

    async fn list_personal_by_owner(
        &self,
        project_id: uuid::Uuid,
        owner_user_id: &str,
    ) -> Result<Vec<Canvas>, DomainError> {
        let rows = sqlx::query_as::<_, CanvasRow>(
            r#"
            SELECT id, project_id, owner_user_id, scope, mount_id, title, description, entry_file,
                   sandbox_config, published_from_canvas_id, shared_canvas_id, cloned_from_canvas_id,
                   published_at, published_by_user_id, created_at, updated_at
            FROM canvases
            WHERE project_id = $1 AND owner_user_id = $2 AND scope = 'personal'
            ORDER BY created_at DESC
            "#,
        )
        .bind(project_id.to_string())
        .bind(owner_user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;

        let canvas_ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let files = self.load_files(&canvas_ids).await?;
        let bindings = self.load_bindings(&canvas_ids).await?;

        rows.into_iter()
            .map(|row| row.try_into_canvas(files.clone(), bindings.clone()))
            .collect()
    }

    async fn list_project_shared(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<Canvas>, DomainError> {
        let rows = sqlx::query_as::<_, CanvasRow>(
            r#"
            SELECT id, project_id, owner_user_id, scope, mount_id, title, description, entry_file,
                   sandbox_config, published_from_canvas_id, shared_canvas_id, cloned_from_canvas_id,
                   published_at, published_by_user_id, created_at, updated_at
            FROM canvases
            WHERE project_id = $1 AND scope = 'project'
            ORDER BY created_at DESC
            "#,
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;

        let canvas_ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let files = self.load_files(&canvas_ids).await?;
        let bindings = self.load_bindings(&canvas_ids).await?;

        rows.into_iter()
            .map(|row| row.try_into_canvas(files.clone(), bindings.clone()))
            .collect()
    }

    async fn find_published_from(
        &self,
        source_canvas_id: uuid::Uuid,
    ) -> Result<Option<Canvas>, DomainError> {
        let row = sqlx::query_as::<_, CanvasRow>(
            r#"
            SELECT id, project_id, owner_user_id, scope, mount_id, title, description, entry_file,
                   sandbox_config, published_from_canvas_id, shared_canvas_id, cloned_from_canvas_id,
                   published_at, published_by_user_id, created_at, updated_at
            FROM canvases
            WHERE published_from_canvas_id = $1 AND scope = 'project'
            ORDER BY published_at DESC NULLS LAST, created_at DESC
            LIMIT 1
            "#,
        )
        .bind(source_canvas_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let canvas_id = row.id.clone();
        let files = self.load_files(std::slice::from_ref(&canvas_id)).await?;
        let bindings = self.load_bindings(std::slice::from_ref(&canvas_id)).await?;
        Ok(Some(row.try_into_canvas(files, bindings)?))
    }

    async fn update(&self, canvas: &Canvas) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(super::db_err)?;
        let published_from_canvas_id = canvas
            .published_from_canvas_id
            .as_ref()
            .map(ToString::to_string);
        let shared_canvas_id = canvas.shared_canvas_id.as_ref().map(ToString::to_string);
        let cloned_from_canvas_id = canvas
            .cloned_from_canvas_id
            .as_ref()
            .map(ToString::to_string);

        let result = sqlx::query(
            r#"
            UPDATE canvases
            SET owner_user_id = $1, scope = $2, mount_id = $3, title = $4, description = $5,
                entry_file = $6, sandbox_config = $7, published_from_canvas_id = $8,
                shared_canvas_id = $9, cloned_from_canvas_id = $10, published_at = $11,
                published_by_user_id = $12, updated_at = $13
            WHERE id = $14
            "#,
        )
        .bind(canvas.owner_user_id.as_deref())
        .bind(canvas.scope.as_str())
        .bind(&canvas.mount_id)
        .bind(&canvas.title)
        .bind(&canvas.description)
        .bind(&canvas.entry_file)
        .bind(serde_json::to_string(&canvas.sandbox_config)?)
        .bind(published_from_canvas_id.as_deref())
        .bind(shared_canvas_id.as_deref())
        .bind(cloned_from_canvas_id.as_deref())
        .bind(canvas.published_at)
        .bind(canvas.published_by_user_id.as_deref())
        .bind(canvas.updated_at)
        .bind(canvas.id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(super::db_err)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "canvas",
                id: canvas.id.to_string(),
            });
        }

        self.replace_files(&mut tx, canvas).await?;
        self.replace_bindings(&mut tx, canvas).await?;

        tx.commit().await.map_err(super::db_err)?;

        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(super::db_err)?;
        let canvas_id = id.to_string();

        sqlx::query("DELETE FROM canvas_bindings WHERE canvas_id = $1")
            .bind(&canvas_id)
            .execute(&mut *tx)
            .await
            .map_err(super::db_err)?;

        sqlx::query("DELETE FROM canvas_files WHERE canvas_id = $1")
            .bind(&canvas_id)
            .execute(&mut *tx)
            .await
            .map_err(super::db_err)?;

        let result = sqlx::query("DELETE FROM canvases WHERE id = $1")
            .bind(&canvas_id)
            .execute(&mut *tx)
            .await
            .map_err(super::db_err)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "canvas",
                id: canvas_id,
            });
        }

        tx.commit().await.map_err(super::db_err)?;

        Ok(())
    }
}

#[derive(Clone, sqlx::FromRow)]
struct CanvasRow {
    id: String,
    project_id: String,
    owner_user_id: Option<String>,
    scope: String,
    mount_id: String,
    title: String,
    description: String,
    entry_file: String,
    sandbox_config: String,
    published_from_canvas_id: Option<String>,
    shared_canvas_id: Option<String>,
    cloned_from_canvas_id: Option<String>,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
    published_by_user_id: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct CanvasFileWithOwnerRow {
    canvas_id: String,
    path: String,
    content: String,
}

#[derive(sqlx::FromRow)]
struct CanvasBindingWithOwnerRow {
    canvas_id: String,
    alias: String,
    source_uri: String,
    content_type: String,
}

impl CanvasRow {
    fn try_into_canvas(
        self,
        files: BTreeMap<String, Vec<CanvasFile>>,
        bindings: BTreeMap<String, Vec<CanvasDataBinding>>,
    ) -> Result<Canvas, DomainError> {
        let sandbox_config = parse_canvas_sandbox_config(&self.sandbox_config)?;
        let files = files.get(&self.id).cloned().ok_or_else(|| {
            DomainError::InvalidConfig(format!("缺少 canvas_files 映射: {}", self.id))
        })?;
        let bindings = bindings.get(&self.id).cloned().ok_or_else(|| {
            DomainError::InvalidConfig(format!("缺少 canvas_bindings 映射: {}", self.id))
        })?;

        Ok(Canvas {
            id: self.id.parse().map_err(|_| DomainError::NotFound {
                entity: "canvas",
                id: self.id.clone(),
            })?,
            project_id: self.project_id.parse().map_err(|_| {
                DomainError::InvalidConfig(String::from("无效的 canvas project_id"))
            })?,
            owner_user_id: self.owner_user_id,
            scope: CanvasScope::parse(&self.scope)?,
            mount_id: self.mount_id,
            title: self.title,
            description: self.description,
            entry_file: self.entry_file,
            sandbox_config,
            files,
            bindings,
            published_from_canvas_id: parse_optional_uuid(
                self.published_from_canvas_id,
                "canvases.published_from_canvas_id",
            )?,
            shared_canvas_id: parse_optional_uuid(
                self.shared_canvas_id,
                "canvases.shared_canvas_id",
            )?,
            cloned_from_canvas_id: parse_optional_uuid(
                self.cloned_from_canvas_id,
                "canvases.cloned_from_canvas_id",
            )?,
            published_at: self.published_at,
            published_by_user_id: self.published_by_user_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn parse_canvas_sandbox_config(raw: &str) -> Result<CanvasSandboxConfig, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("canvases.sandbox_config: {error}")))
}

fn parse_optional_uuid(
    raw: Option<String>,
    column: &str,
) -> Result<Option<uuid::Uuid>, DomainError> {
    raw.map(|value| {
        value
            .parse()
            .map_err(|_| DomainError::InvalidConfig(format!("{column}: UUID 无效")))
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::persistence::postgres::test_pg_pool;

    async fn new_repo() -> Option<PostgresCanvasRepository> {
        let pool = match test_pg_pool("canvas_repository").await {
            Some(pool) => pool,
            None => return None,
        };
        let repo = PostgresCanvasRepository::new(pool);
        repo.initialize().await.expect("应能初始化 canvas schema");
        Some(repo)
    }

    #[tokio::test]
    async fn create_and_get_canvas_roundtrip() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let project_id = Uuid::new_v4();
        let mut canvas = Canvas::new_personal(
            project_id,
            "alice".to_string(),
            "cvs-demo".to_string(),
            "Demo".to_string(),
            "desc".to_string(),
        );
        canvas.cloned_from_canvas_id = Some(Uuid::new_v4());
        canvas.bindings = vec![CanvasDataBinding::new(
            "stats".to_string(),
            "lifecycle://active/artifacts/1".to_string(),
        )];

        CanvasRepository::create(&repo, &canvas)
            .await
            .expect("应能创建 canvas");

        let persisted = CanvasRepository::get_by_id(&repo, canvas.id)
            .await
            .expect("应能读取 canvas")
            .expect("canvas 应存在");

        assert_eq!(persisted.project_id, project_id);
        assert_eq!(persisted.owner_user_id.as_deref(), Some("alice"));
        assert_eq!(persisted.scope, CanvasScope::Personal);
        assert_eq!(
            persisted.cloned_from_canvas_id,
            canvas.cloned_from_canvas_id
        );
        assert!(
            persisted
                .files
                .iter()
                .all(|file| !file.path.starts_with("skills/canvas-system/"))
        );
        assert_eq!(persisted.bindings.len(), 1);
    }

    #[tokio::test]
    async fn update_canvas_replaces_files_and_bindings() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let mut canvas = Canvas::new_personal(
            Uuid::new_v4(),
            "alice".to_string(),
            "cvs-demo-update".to_string(),
            "Demo".to_string(),
            String::new(),
        );
        CanvasRepository::create(&repo, &canvas)
            .await
            .expect("应能创建 canvas");

        canvas.files = vec![CanvasFile::new(
            "src/main.ts".to_string(),
            "console.log('updated')".to_string(),
        )];
        canvas.entry_file = "src/main.ts".to_string();
        canvas.bindings = vec![CanvasDataBinding::new(
            "summary".to_string(),
            "lifecycle://active/artifacts/2".to_string(),
        )];
        canvas.touch();

        CanvasRepository::update(&repo, &canvas)
            .await
            .expect("应能更新 canvas");

        let persisted = CanvasRepository::get_by_id(&repo, canvas.id)
            .await
            .expect("应能读取 canvas")
            .expect("canvas 应存在");

        assert_eq!(persisted.entry_file, "src/main.ts");
        assert_eq!(persisted.files[0].path, "src/main.ts");
        assert_eq!(persisted.bindings[0].alias, "summary");
    }

    #[tokio::test]
    async fn list_scope_queries_and_find_published_from_use_lineage_fields() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let project_id = Uuid::new_v4();
        let mut source = Canvas::new_personal(
            project_id,
            "alice".to_string(),
            "cvs-source".to_string(),
            "Source".to_string(),
            "desc".to_string(),
        );
        CanvasRepository::create(&repo, &source)
            .await
            .expect("应能创建 source");

        let mut shared = Canvas::new_project_shared(
            project_id,
            "cvs-source-shared".to_string(),
            "Source".to_string(),
            "desc".to_string(),
            Some(source.id),
            Some("alice".to_string()),
        );
        shared.copy_authoring_from(&source);
        CanvasRepository::create(&repo, &shared)
            .await
            .expect("应能创建 shared canvas");

        source.shared_canvas_id = Some(shared.id);
        source.touch();
        CanvasRepository::update(&repo, &source)
            .await
            .expect("应能更新 source lineage");

        let mine = CanvasRepository::list_personal_by_owner(&repo, project_id, "alice")
            .await
            .expect("应能查询个人 canvas");
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].id, source.id);
        assert_eq!(mine[0].shared_canvas_id, Some(shared.id));

        let shared_list = CanvasRepository::list_project_shared(&repo, project_id)
            .await
            .expect("应能查询项目共用 canvas");
        assert_eq!(shared_list.len(), 1);
        assert_eq!(shared_list[0].id, shared.id);
        assert_eq!(shared_list[0].published_from_canvas_id, Some(source.id));
        assert_eq!(
            shared_list[0].published_by_user_id.as_deref(),
            Some("alice")
        );

        let published = CanvasRepository::find_published_from(&repo, source.id)
            .await
            .expect("应能查询发布 lineage")
            .expect("发布记录应存在");
        assert_eq!(published.id, shared.id);
    }

    #[tokio::test]
    async fn delete_canvas_removes_files_and_bindings() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let mut canvas = Canvas::new_personal(
            Uuid::new_v4(),
            "alice".to_string(),
            "cvs-delete".to_string(),
            "Delete".to_string(),
            String::new(),
        );
        canvas.files = vec![CanvasFile::new(
            "src/main.tsx".to_string(),
            "export default function App() { return null }".to_string(),
        )];
        canvas.bindings = vec![CanvasDataBinding::new(
            "stats".to_string(),
            "lifecycle://active/artifacts/stats".to_string(),
        )];
        CanvasRepository::create(&repo, &canvas)
            .await
            .expect("应能创建 canvas");

        CanvasRepository::delete(&repo, canvas.id)
            .await
            .expect("应能删除 canvas");

        let canvas_id = canvas.id.to_string();
        let file_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM canvas_files WHERE canvas_id = $1")
                .bind(&canvas_id)
                .fetch_one(&repo.pool)
                .await
                .expect("应能查询 canvas_files");
        let binding_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM canvas_bindings WHERE canvas_id = $1")
                .bind(&canvas_id)
                .fetch_one(&repo.pool)
                .await
                .expect("应能查询 canvas_bindings");

        assert_eq!(file_count, 0);
        assert_eq!(binding_count, 0);
    }
}
