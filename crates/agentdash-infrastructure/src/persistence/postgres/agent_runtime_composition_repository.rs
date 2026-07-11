use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBinding, AgentRunRuntimeBindingError, AgentRunRuntimeBindingRepository,
    AgentRunRuntimeTarget,
};
use agentdash_integration_api::{
    AgentRuntimeSurfaceBroker, DriverSurfaceError, DriverSurfaceRequest, DriverToolSurface,
    MaterializedDriverSurface,
};
use async_trait::async_trait;
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PostgresAgentRuntimeCompositionRepository {
    pool: PgPool,
}

impl PostgresAgentRuntimeCompositionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn load_by_runtime_binding(
        &self,
        binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        let row = sqlx::query(
            "SELECT binding FROM agent_run_runtime_binding WHERE runtime_binding_id=$1",
        )
        .bind(binding_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(binding_sql_error)?;
        row.map(|row| {
            serde_json::from_value(row.get("binding")).map_err(|error| {
                AgentRunRuntimeBindingError::Persistence {
                    reason: format!("agent_run_runtime_binding.binding: {error}"),
                }
            })
        })
        .transpose()
    }

    pub async fn put_surface(
        &self,
        binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
        surface: &MaterializedDriverSurface,
    ) -> Result<(), DriverSurfaceError> {
        let materialized = serde_json::to_value(surface).map_err(|error| {
            DriverSurfaceError::InvalidMaterialization {
                reason: error.to_string(),
            }
        })?;
        let result = sqlx::query(
            "INSERT INTO agent_runtime_surface_snapshot \
             (binding_id,surface_revision,surface_digest,tool_set_revision,tool_set_digest,hook_plan_revision,hook_plan_digest,materialized) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (binding_id) DO NOTHING",
        )
        .bind(binding_id.as_str())
        .bind(i64::try_from(surface.revision.0).map_err(|_| invalid_surface("surface revision"))?)
        .bind(surface.digest.as_str())
        .bind(i64::try_from(surface.tools.revision.0).map_err(|_| invalid_surface("tool revision"))?)
        .bind(&surface.tools.digest)
        .bind(i64::try_from(surface.hooks.revision.0).map_err(|_| invalid_surface("hook revision"))?)
        .bind(surface.hooks.digest.as_str())
        .bind(materialized.clone())
        .execute(&self.pool)
        .await
        .map_err(surface_sql_error)?;
        if result.rows_affected() == 0 {
            let existing: serde_json::Value = sqlx::query_scalar(
                "SELECT materialized FROM agent_runtime_surface_snapshot WHERE binding_id=$1",
            )
            .bind(binding_id.as_str())
            .fetch_one(&self.pool)
            .await
            .map_err(surface_sql_error)?;
            if existing != materialized {
                return Err(DriverSurfaceError::InvalidMaterialization {
                    reason: "binding surface identity was reused with different content"
                        .to_string(),
                });
            }
        }
        Ok(())
    }

    pub async fn load_bound_surface(
        &self,
        binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
    ) -> Result<Option<MaterializedDriverSurface>, DriverSurfaceError> {
        let row = sqlx::query(
            "SELECT materialized FROM agent_runtime_surface_snapshot WHERE binding_id=$1",
        )
        .bind(binding_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(surface_sql_error)?;
        row.map(|row| {
            serde_json::from_value(row.get("materialized")).map_err(|error| {
                DriverSurfaceError::InvalidMaterialization {
                    reason: format!("agent_runtime_surface_snapshot.materialized: {error}"),
                }
            })
        })
        .transpose()
    }
}

#[async_trait]
impl AgentRuntimeSurfaceBroker for PostgresAgentRuntimeCompositionRepository {
    async fn materialize(
        &self,
        request: DriverSurfaceRequest,
    ) -> Result<MaterializedDriverSurface, DriverSurfaceError> {
        let surface = self
            .load_bound_surface(&request.binding_id)
            .await?
            .ok_or_else(|| DriverSurfaceError::Unavailable {
                reason: "bound surface does not exist".to_string(),
                retryable: false,
            })?;
        if surface.revision != request.surface_revision || surface.digest != request.surface_digest
        {
            return Err(DriverSurfaceError::Stale);
        }
        Ok(surface)
    }

    async fn materialize_tool_set(
        &self,
        binding_id: agentdash_agent_runtime_contract::RuntimeBindingId,
        revision: agentdash_agent_runtime_contract::ToolSetRevision,
        digest: &str,
    ) -> Result<DriverToolSurface, DriverSurfaceError> {
        let surface = self.load_bound_surface(&binding_id).await?.ok_or_else(|| {
            DriverSurfaceError::Unavailable {
                reason: "bound surface does not exist".to_string(),
                retryable: false,
            }
        })?;
        if surface.tools.revision != revision || surface.tools.digest != digest {
            return Err(DriverSurfaceError::Stale);
        }
        Ok(surface.tools)
    }
}

#[async_trait]
impl AgentRunRuntimeBindingRepository for PostgresAgentRuntimeCompositionRepository {
    async fn load(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        let row = sqlx::query(
            "SELECT binding FROM agent_run_runtime_binding WHERE run_id=$1 AND agent_id=$2",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(binding_sql_error)?;
        row.map(|row| {
            serde_json::from_value(row.get("binding")).map_err(|error| {
                AgentRunRuntimeBindingError::Persistence {
                    reason: format!("agent_run_runtime_binding.binding: {error}"),
                }
            })
        })
        .transpose()
    }

    async fn load_by_thread_id(
        &self,
        thread_id: &agentdash_agent_runtime_contract::RuntimeThreadId,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        let row =
            sqlx::query("SELECT binding FROM agent_run_runtime_binding WHERE runtime_thread_id=$1")
                .bind(thread_id.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(binding_sql_error)?;
        row.map(|row| {
            serde_json::from_value(row.get("binding")).map_err(|error| {
                AgentRunRuntimeBindingError::Persistence {
                    reason: format!("agent_run_runtime_binding.binding: {error}"),
                }
            })
        })
        .transpose()
    }

    async fn list_by_run(
        &self,
        run_id: uuid::Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        load_runtime_bindings(
            sqlx::query("SELECT binding FROM agent_run_runtime_binding WHERE run_id=$1")
                .bind(run_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(binding_sql_error)?,
        )
    }

    async fn list_by_agent(
        &self,
        agent_id: uuid::Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        load_runtime_bindings(
            sqlx::query("SELECT binding FROM agent_run_runtime_binding WHERE agent_id=$1")
                .bind(agent_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(binding_sql_error)?,
        )
    }

    async fn insert(
        &self,
        binding: AgentRunRuntimeBinding,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        let document = serde_json::to_value(&binding).map_err(|error| {
            AgentRunRuntimeBindingError::Persistence {
                reason: error.to_string(),
            }
        })?;
        let result = sqlx::query(
            "INSERT INTO agent_run_runtime_binding \
             (run_id,agent_id,runtime_thread_id,runtime_binding_id,binding) \
             VALUES ($1,$2,$3,$4,$5) ON CONFLICT (run_id,agent_id) DO NOTHING",
        )
        .bind(binding.target.run_id.to_string())
        .bind(binding.target.agent_id.to_string())
        .bind(binding.thread_id.as_str())
        .bind(binding.binding_id.as_str())
        .bind(document.clone())
        .execute(&self.pool)
        .await
        .map_err(binding_sql_error)?;
        if result.rows_affected() == 0 {
            let existing = self
                .load(&binding.target)
                .await?
                .ok_or(AgentRunRuntimeBindingError::Conflict)?;
            if existing != binding {
                return Err(AgentRunRuntimeBindingError::Conflict);
            }
            return Ok(existing);
        }
        Ok(binding)
    }
}

fn load_runtime_bindings(
    rows: Vec<sqlx::postgres::PgRow>,
) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
    rows.into_iter()
        .map(|row| {
            serde_json::from_value(row.get("binding")).map_err(|error| {
                AgentRunRuntimeBindingError::Persistence {
                    reason: format!("agent_run_runtime_binding.binding: {error}"),
                }
            })
        })
        .collect()
}

fn invalid_surface(field: &'static str) -> DriverSurfaceError {
    DriverSurfaceError::InvalidMaterialization {
        reason: format!("{field} exceeds PostgreSQL bigint"),
    }
}

fn surface_sql_error(error: sqlx::Error) -> DriverSurfaceError {
    DriverSurfaceError::Unavailable {
        reason: error.to_string(),
        retryable: true,
    }
}

fn binding_sql_error(error: sqlx::Error) -> AgentRunRuntimeBindingError {
    if let sqlx::Error::Database(database) = &error
        && matches!(database.code().as_deref(), Some("23505" | "23503"))
    {
        return AgentRunRuntimeBindingError::Conflict;
    }
    AgentRunRuntimeBindingError::Persistence {
        reason: error.to_string(),
    }
}
