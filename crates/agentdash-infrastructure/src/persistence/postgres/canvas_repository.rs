use std::collections::BTreeMap;

use sqlx::PgPool;

use agentdash_domain::DomainError;
use agentdash_domain::canvas::{
    Canvas, CanvasDataBinding, CanvasFile, CanvasImportMap, CanvasRepository, CanvasSandboxConfig,
};

pub struct SqliteCanvasRepository {
    pool: PgPool,
}

impl SqliteCanvasRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS canvases (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                mount_id TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                entry_file TEXT NOT NULL,
                sandbox_config TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.ensure_mount_id_column().await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS canvas_files (
                canvas_id TEXT NOT NULL,
                path TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (canvas_id, path),
                FOREIGN KEY(canvas_id) REFERENCES canvases(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS canvas_bindings (
                canvas_id TEXT NOT NULL,
                alias TEXT NOT NULL,
                source_uri TEXT NOT NULL,
                content_type TEXT NOT NULL DEFAULT 'application/json',
                PRIMARY KEY (canvas_id, alias),
                FOREIGN KEY(canvas_id) REFERENCES canvases(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_canvases_project_id ON canvases(project_id)")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_canvases_project_mount_id ON canvases(project_id, mount_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn ensure_mount_id_column(&self) -> Result<(), DomainError> {
        let columns = sqlx::query_scalar::<_, String>(
            "SELECT column_name
             FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = 'canvases'",
        )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        if columns.iter().any(|column| column == "mount_id") {
            return Ok(());
        }

        sqlx::query("ALTER TABLE canvases ADD COLUMN mount_id TEXT NOT NULL DEFAULT ''")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        sqlx::query("UPDATE canvases SET mount_id = id WHERE mount_id = ''")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn load_files(
        &self,
        canvas_ids: &[String],
    ) -> Result<BTreeMap<String, Vec<CanvasFile>>, DomainError> {
        let mut file_map = BTreeMap::<String, Vec<CanvasFile>>::new();
        for canvas_id in canvas_ids {
            let rows = sqlx::query_as::<_, CanvasFileRow>(
                r#"
                SELECT path, content
                FROM canvas_files
                WHERE canvas_id = $1
                ORDER BY path ASC
                "#,
            )
            .bind(canvas_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

            file_map.insert(
                canvas_id.clone(),
                rows.into_iter()
                    .map(|row| CanvasFile::new(row.path, row.content))
                    .collect(),
            );
        }

        Ok(file_map)
    }

    async fn load_bindings(
        &self,
        canvas_ids: &[String],
    ) -> Result<BTreeMap<String, Vec<CanvasDataBinding>>, DomainError> {
        let mut binding_map = BTreeMap::<String, Vec<CanvasDataBinding>>::new();
        for canvas_id in canvas_ids {
            let rows = sqlx::query_as::<_, CanvasBindingRow>(
                r#"
                SELECT alias, source_uri, content_type
                FROM canvas_bindings
                WHERE canvas_id = $1
                ORDER BY alias ASC
                "#,
            )
            .bind(canvas_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

            binding_map.insert(
                canvas_id.clone(),
                rows.into_iter()
                    .map(|row| CanvasDataBinding {
                        alias: row.alias,
                        source_uri: row.source_uri,
                        content_type: row.content_type,
                    })
                    .collect(),
            );
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
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        for file in &canvas.files {
            sqlx::query("INSERT INTO canvas_files (canvas_id, path, content) VALUES ($1, $2, $3)")
                .bind(canvas.id.to_string())
                .bind(&file.path)
                .bind(&file.content)
                .execute(&mut **tx)
                .await
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        }

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
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        for binding in &canvas.bindings {
            sqlx::query(
                "INSERT INTO canvas_bindings (canvas_id, alias, source_uri, content_type) VALUES ($1, $2, $3, $4)",
            )
            .bind(canvas.id.to_string())
            .bind(&binding.alias)
            .bind(&binding.source_uri)
            .bind(&binding.content_type)
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl CanvasRepository for SqliteCanvasRepository {
    async fn create(&self, canvas: &Canvas) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO canvases (
                id, project_id, mount_id, title, description, entry_file, sandbox_config, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(canvas.id.to_string())
        .bind(canvas.project_id.to_string())
        .bind(&canvas.mount_id)
        .bind(&canvas.title)
        .bind(&canvas.description)
        .bind(&canvas.entry_file)
        .bind(serde_json::to_string(&canvas.sandbox_config)?)
        .bind(canvas.created_at.to_rfc3339())
        .bind(canvas.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.replace_files(&mut tx, canvas).await?;
        self.replace_bindings(&mut tx, canvas).await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<Canvas>, DomainError> {
        let row = sqlx::query_as::<_, CanvasRow>(
            r#"
            SELECT id, project_id, mount_id, title, description, entry_file, sandbox_config, created_at, updated_at
            FROM canvases
            WHERE id = $1
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
            SELECT id, project_id, mount_id, title, description, entry_file, sandbox_config, created_at, updated_at
            FROM canvases
            WHERE project_id = $1 AND mount_id = $2
            "#,
        )
        .bind(project_id.to_string())
        .bind(mount_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
            SELECT id, project_id, mount_id, title, description, entry_file, sandbox_config, created_at, updated_at
            FROM canvases
            WHERE project_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let canvas_ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let files = self.load_files(&canvas_ids).await?;
        let bindings = self.load_bindings(&canvas_ids).await?;

        rows.into_iter()
            .map(|row| row.try_into_canvas(files.clone(), bindings.clone()))
            .collect()
    }

    async fn update(&self, canvas: &Canvas) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let result = sqlx::query(
            r#"
            UPDATE canvases
            SET mount_id = $1, title = $2, description = $3, entry_file = $4, sandbox_config = $5, updated_at = $6
            WHERE id = $7
            "#,
        )
        .bind(&canvas.mount_id)
        .bind(&canvas.title)
        .bind(&canvas.description)
        .bind(&canvas.entry_file)
        .bind(serde_json::to_string(&canvas.sandbox_config)?)
        .bind(canvas.updated_at.to_rfc3339())
        .bind(canvas.id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "canvas",
                id: canvas.id.to_string(),
            });
        }

        self.replace_files(&mut tx, canvas).await?;
        self.replace_bindings(&mut tx, canvas).await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM canvases WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "canvas",
                id: id.to_string(),
            });
        }

        Ok(())
    }
}

#[derive(Clone, sqlx::FromRow)]
struct CanvasRow {
    id: String,
    project_id: String,
    mount_id: String,
    title: String,
    description: String,
    entry_file: String,
    sandbox_config: String,
    created_at: String,
    updated_at: String,
}

#[derive(sqlx::FromRow)]
struct CanvasFileRow {
    path: String,
    content: String,
}

#[derive(sqlx::FromRow)]
struct CanvasBindingRow {
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
        let sandbox_config = serde_json::from_str::<CanvasSandboxConfig>(&self.sandbox_config)
            .unwrap_or(CanvasSandboxConfig {
                libraries: Vec::new(),
                import_map: CanvasImportMap::default(),
            });

        Ok(Canvas {
            id: self.id.parse().map_err(|_| DomainError::NotFound {
                entity: "canvas",
                id: self.id.clone(),
            })?,
            project_id: self
                .project_id
                .parse()
                .map_err(|_| DomainError::InvalidConfig("无效的 canvas project_id".to_string()))?,
            mount_id: self.mount_id,
            title: self.title,
            description: self.description,
            entry_file: self.entry_file,
            sandbox_config,
            files: files.get(&self.id).cloned().unwrap_or_default(),
            bindings: bindings.get(&self.id).cloned().unwrap_or_default(),
            created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&self.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::*;

    async fn new_repo() -> SqliteCanvasRepository {
        let database_url =
            std::env::var("TEST_DATABASE_URL").expect("运行测试前需设置 TEST_DATABASE_URL");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("应能连接测试 PostgreSQL");
        let repo = SqliteCanvasRepository::new(pool);
        repo.initialize().await.expect("应能初始化 canvas schema");
        repo
    }

    #[tokio::test]
    async fn create_and_get_canvas_roundtrip() {
        let repo = new_repo().await;
        let project_id = Uuid::new_v4();
        let mut canvas = Canvas::new(
            project_id,
            "demo".to_string(),
            "Demo".to_string(),
            "desc".to_string(),
        );
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
        assert_eq!(persisted.files.len(), 1);
        assert_eq!(persisted.bindings.len(), 1);
    }

    #[tokio::test]
    async fn update_canvas_replaces_files_and_bindings() {
        let repo = new_repo().await;
        let mut canvas = Canvas::new(
            Uuid::new_v4(),
            "demo".to_string(),
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
}
