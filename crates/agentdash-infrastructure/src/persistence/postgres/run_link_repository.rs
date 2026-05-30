use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    LifecycleRunLink, LifecycleRunLinkRepository, RunLinkRole, RunLinkSubjectKind,
};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::db_err;

const LINK_COLS: &str = "id,run_id,subject_kind,subject_id,role,metadata,created_at";

pub struct PostgresRunLinkRepository {
    pool: PgPool,
}

impl PostgresRunLinkRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl LifecycleRunLinkRepository for PostgresRunLinkRepository {
    async fn create(&self, link: &LifecycleRunLink) -> Result<(), DomainError> {
        sqlx::query(&format!(
            "INSERT INTO lifecycle_run_links ({LINK_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7)"
        ))
        .bind(link.id.to_string())
        .bind(link.run_id.to_string())
        .bind(link.subject_kind.as_str())
        .bind(link.subject_id.to_string())
        .bind(link.role.as_str())
        .bind(link.metadata.clone())
        .bind(link.created_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleRunLink>, DomainError> {
        sqlx::query_as::<_, RunLinkRow>(&format!(
            "SELECT {LINK_COLS} FROM lifecycle_run_links WHERE run_id = $1 ORDER BY created_at"
        ))
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn list_by_subject(
        &self,
        subject_kind: RunLinkSubjectKind,
        subject_id: Uuid,
    ) -> Result<Vec<LifecycleRunLink>, DomainError> {
        sqlx::query_as::<_, RunLinkRow>(&format!(
            "SELECT {LINK_COLS} FROM lifecycle_run_links WHERE subject_kind = $1 AND subject_id = $2 ORDER BY created_at DESC"
        ))
        .bind(subject_kind.as_str())
        .bind(subject_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn list_by_subject_and_role(
        &self,
        subject_kind: RunLinkSubjectKind,
        subject_id: Uuid,
        role: RunLinkRole,
    ) -> Result<Vec<LifecycleRunLink>, DomainError> {
        sqlx::query_as::<_, RunLinkRow>(&format!(
            "SELECT {LINK_COLS} FROM lifecycle_run_links WHERE subject_kind = $1 AND subject_id = $2 AND role = $3 ORDER BY created_at DESC"
        ))
        .bind(subject_kind.as_str())
        .bind(subject_id.to_string())
        .bind(role.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM lifecycle_run_links WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn delete_by_run(&self, run_id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM lifecycle_run_links WHERE run_id = $1")
            .bind(run_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct RunLinkRow {
    id: String,
    run_id: String,
    subject_kind: String,
    subject_id: String,
    role: String,
    metadata: Option<serde_json::Value>,
    created_at: DateTime<Utc>,
}

fn parse_uuid(s: &str, ctx: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(s).map_err(|e| {
        DomainError::InvalidConfig(format!(
            "lifecycle_run_links.{ctx}: invalid uuid `{s}`: {e}"
        ))
    })
}

impl TryFrom<RunLinkRow> for LifecycleRunLink {
    type Error = DomainError;
    fn try_from(row: RunLinkRow) -> Result<Self, Self::Error> {
        let subject_kind = RunLinkSubjectKind::parse(&row.subject_kind).ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "lifecycle_run_links.subject_kind: unknown value `{}`",
                row.subject_kind
            ))
        })?;
        let role = RunLinkRole::parse(&row.role).ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "lifecycle_run_links.role: unknown value `{}`",
                row.role
            ))
        })?;
        Ok(LifecycleRunLink {
            id: parse_uuid(&row.id, "id")?,
            run_id: parse_uuid(&row.run_id, "run_id")?,
            subject_kind,
            subject_id: parse_uuid(&row.subject_id, "subject_id")?,
            role,
            metadata: row.metadata,
            created_at: row.created_at,
        })
    }
}
