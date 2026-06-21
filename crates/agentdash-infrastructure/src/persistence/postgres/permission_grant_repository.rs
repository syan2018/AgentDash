use agentdash_domain::common::error::DomainError;
use agentdash_domain::permission::{
    GrantScope, GrantStatus, PermissionGrant, PermissionGrantRepository,
    PermissionGrantStatusFilter, PolicyDecision, ScopeEscalationIntent,
};
use agentdash_domain::workflow::ToolCapabilityPath;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use super::db_err;

const TABLE: &str = "permission_grants";

pub struct PostgresPermissionGrantRepository {
    pool: PgPool,
}

impl PostgresPermissionGrantRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl PermissionGrantRepository for PostgresPermissionGrantRepository {
    async fn create(&self, grant: &PermissionGrant) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO permission_grants \
             (id, run_id, effect_frame_id, source_runtime_session_id, \
              source_turn_id, source_tool_call_id, \
              requested_paths, reason, grant_scope, expires_at, \
              scope_escalation_intent, status, policy_decision, approved_by, \
              created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
        )
        .bind(grant.id.to_string())
        .bind(grant.run_id.to_string())
        .bind(grant.effect_frame_id.map(|id| id.to_string()))
        .bind(&grant.source_runtime_session_id)
        .bind(&grant.source_turn_id)
        .bind(&grant.source_tool_call_id)
        .bind(serde_json::to_value(&grant.requested_paths).map_err(DomainError::Serialization)?)
        .bind(&grant.reason)
        .bind(grant.grant_scope.as_str())
        .bind(grant.expires_at)
        .bind(
            grant
                .scope_escalation_intent
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(DomainError::Serialization)?,
        )
        .bind(grant.status.as_str())
        .bind(
            grant
                .policy_decision
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(DomainError::Serialization)?,
        )
        .bind(&grant.approved_by)
        .bind(grant.created_at)
        .bind(grant.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn update(&self, grant: &PermissionGrant) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE permission_grants SET \
             status=$2, policy_decision=$3, approved_by=$4, \
             scope_escalation_intent=$5, updated_at=$6 \
             WHERE id=$1",
        )
        .bind(grant.id.to_string())
        .bind(grant.status.as_str())
        .bind(
            grant
                .policy_decision
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(DomainError::Serialization)?,
        )
        .bind(&grant.approved_by)
        .bind(
            grant
                .scope_escalation_intent
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(DomainError::Serialization)?,
        )
        .bind(grant.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<PermissionGrant>, DomainError> {
        let row = sqlx::query_as::<_, GrantRow>("SELECT * FROM permission_grants WHERE id = $1")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_by_frame(
        &self,
        effect_frame_id: Uuid,
        status_filter: Option<PermissionGrantStatusFilter>,
    ) -> Result<Vec<PermissionGrant>, DomainError> {
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT * FROM permission_grants WHERE effect_frame_id = ",
        );
        query.push_bind(effect_frame_id.to_string());
        push_status_filter(&mut query, status_filter);
        query.push(" ORDER BY created_at DESC");

        query
            .build_query_as::<GrantRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?
            .into_iter()
            .map(TryInto::try_into)
            .collect()
    }

    async fn list_by_run(
        &self,
        run_id: Uuid,
        status_filter: Option<PermissionGrantStatusFilter>,
    ) -> Result<Vec<PermissionGrant>, DomainError> {
        let mut query =
            QueryBuilder::<Postgres>::new("SELECT * FROM permission_grants WHERE run_id = ");
        query.push_bind(run_id.to_string());
        push_status_filter(&mut query, status_filter);
        query.push(" ORDER BY created_at DESC");

        query
            .build_query_as::<GrantRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?
            .into_iter()
            .map(TryInto::try_into)
            .collect()
    }

    async fn list_active_by_frame(
        &self,
        effect_frame_id: Uuid,
    ) -> Result<Vec<PermissionGrant>, DomainError> {
        self.list_by_frame(effect_frame_id, Some(PermissionGrantStatusFilter::Active))
            .await
    }

    async fn list_active_by_run(&self, run_id: Uuid) -> Result<Vec<PermissionGrant>, DomainError> {
        self.list_by_run(run_id, Some(PermissionGrantStatusFilter::Active))
            .await
    }

    async fn find_active_escalation_grant(
        &self,
        effect_frame_id: Uuid,
        target_subject_kind: &str,
    ) -> Result<Option<PermissionGrant>, DomainError> {
        let row = sqlx::query_as::<_, GrantRow>(
            "SELECT * FROM permission_grants \
             WHERE effect_frame_id = $1 \
               AND status = 'applied' \
               AND scope_escalation_intent IS NOT NULL \
               AND scope_escalation_intent LIKE $2 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(effect_frame_id.to_string())
        .bind(format!(
            "%\"target_subject_kind\":\"{target_subject_kind}\"%"
        ))
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_overdue_active(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<PermissionGrant>, DomainError> {
        sqlx::query_as::<_, GrantRow>(
            "SELECT * FROM permission_grants \
             WHERE status IN ('applied', 'scope_escalated') \
               AND expires_at IS NOT NULL \
               AND expires_at < $1 \
             ORDER BY expires_at ASC, created_at ASC",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }
}

fn push_status_filter(
    query: &mut QueryBuilder<'_, Postgres>,
    status_filter: Option<PermissionGrantStatusFilter>,
) {
    match status_filter {
        Some(PermissionGrantStatusFilter::Exact(status)) => {
            query.push(" AND status = ");
            query.push_bind(status.as_str());
        }
        Some(PermissionGrantStatusFilter::Pending) => {
            query.push(
                " AND status IN ('created', 'pending_policy', 'pending_user_approval', 'approved')",
            );
        }
        Some(PermissionGrantStatusFilter::Active) => {
            query.push(" AND status IN ('applied', 'scope_escalated')");
        }
        Some(PermissionGrantStatusFilter::Terminal) => {
            query.push(" AND status IN ('rejected', 'failed', 'expired', 'revoked')");
        }
        None => {}
    }
}

#[derive(sqlx::FromRow)]
struct GrantRow {
    id: String,
    run_id: String,
    effect_frame_id: Option<String>,
    source_runtime_session_id: String,
    source_turn_id: Option<String>,
    source_tool_call_id: Option<String>,
    requested_paths: serde_json::Value,
    reason: String,
    grant_scope: String,
    expires_at: Option<DateTime<Utc>>,
    scope_escalation_intent: Option<serde_json::Value>,
    status: String,
    policy_decision: Option<serde_json::Value>,
    approved_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<GrantRow> for PermissionGrant {
    type Error = DomainError;

    fn try_from(row: GrantRow) -> Result<Self, Self::Error> {
        let id = parse_uuid(&row.id, "id")?;
        let run_id = parse_uuid(&row.run_id, "run_id")?;
        let effect_frame_id = row
            .effect_frame_id
            .as_deref()
            .map(|s| parse_uuid(s, "effect_frame_id"))
            .transpose()?;
        let requested_paths: Vec<ToolCapabilityPath> = serde_json::from_value(row.requested_paths)
            .map_err(|e| DomainError::InvalidConfig(format!("{TABLE}.requested_paths: {e}")))?;
        let grant_scope = GrantScope::parse(&row.grant_scope).ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "{TABLE}.grant_scope: unknown value `{}`",
                row.grant_scope
            ))
        })?;
        let status = GrantStatus::parse(&row.status).ok_or_else(|| {
            DomainError::InvalidConfig(format!("{TABLE}.status: unknown value `{}`", row.status))
        })?;
        let scope_escalation_intent: Option<ScopeEscalationIntent> = row
            .scope_escalation_intent
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| {
                DomainError::InvalidConfig(format!("{TABLE}.scope_escalation_intent: {e}"))
            })?;
        let policy_decision: Option<PolicyDecision> = row
            .policy_decision
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| DomainError::InvalidConfig(format!("{TABLE}.policy_decision: {e}")))?;

        Ok(PermissionGrant {
            id,
            run_id,
            effect_frame_id,
            source_runtime_session_id: row.source_runtime_session_id,
            source_turn_id: row.source_turn_id,
            source_tool_call_id: row.source_tool_call_id,
            requested_paths,
            reason: row.reason,
            grant_scope,
            expires_at: row.expires_at,
            scope_escalation_intent,
            status,
            policy_decision,
            approved_by: row.approved_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

fn parse_uuid(s: &str, field: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(s).map_err(|e| {
        DomainError::InvalidConfig(format!("{TABLE}.{field}: invalid uuid `{s}`: {e}"))
    })
}
