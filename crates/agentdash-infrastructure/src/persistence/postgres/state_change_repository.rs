use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::story::{ChangeKind, StateChange, StateChangeRepository};

use super::state_change_store::{
    append_state_change, get_state_changes_since, get_state_changes_since_by_project,
    initialize_state_changes_schema, latest_state_change_id, latest_state_change_id_by_project,
};

pub struct PostgresStateChangeRepository {
    pool: PgPool,
}

impl PostgresStateChangeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        initialize_state_changes_schema(&self.pool).await
    }
}

#[async_trait::async_trait]
impl StateChangeRepository for PostgresStateChangeRepository {
    async fn get_changes_since(
        &self,
        since_id: i64,
        limit: i64,
    ) -> Result<Vec<StateChange>, DomainError> {
        get_state_changes_since(&self.pool, since_id, limit).await
    }

    async fn get_changes_since_by_project(
        &self,
        project_id: uuid::Uuid,
        since_id: i64,
        limit: i64,
    ) -> Result<Vec<StateChange>, DomainError> {
        get_state_changes_since_by_project(&self.pool, project_id, since_id, limit).await
    }

    async fn latest_event_id(&self) -> Result<i64, DomainError> {
        latest_state_change_id(&self.pool).await
    }

    async fn latest_event_id_by_project(&self, project_id: uuid::Uuid) -> Result<i64, DomainError> {
        latest_state_change_id_by_project(&self.pool, project_id).await
    }

    async fn append_change(
        &self,
        project_id: uuid::Uuid,
        entity_id: uuid::Uuid,
        kind: ChangeKind,
        payload: serde_json::Value,
        backend_id: Option<&str>,
    ) -> Result<(), DomainError> {
        append_state_change(&self.pool, project_id, entity_id, kind, payload, backend_id).await
    }
}
