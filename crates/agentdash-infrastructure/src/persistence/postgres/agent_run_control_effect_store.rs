use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectKind, AgentRunControlEffectRecord, AgentRunControlEffectStatus,
    AgentRunControlEffectStore, NewAgentRunControlEffectRecord,
};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresAgentRunControlEffectStore {
    pool: PgPool,
}

impl PostgresAgentRunControlEffectStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentRunControlEffectStore for PostgresAgentRunControlEffectStore {
    async fn insert_or_get(
        &self,
        effect: NewAgentRunControlEffectRecord,
    ) -> Result<AgentRunControlEffectRecord, String> {
        let now = chrono::Utc::now().timestamp_millis();
        let id = Uuid::new_v4();
        let terminal_event_sequence = i64::try_from(effect.terminal_event_sequence.0)
            .map_err(|_| "terminal event sequence exceeds PostgreSQL bigint".to_string())?;
        let row = sqlx::query(
            r#"
            INSERT INTO agent_run_control_effects (
                id, dedup_key, run_id, agent_id, frame_id, delivery_runtime_session_id, turn_id,
                terminal_event_seq, effect_kind, payload_json, status, attempt_count,
                claim_token, claim_owner, claim_expires_at_ms, created_at_ms, updated_at_ms,
                last_error
            ) VALUES ($1, $2, NULL, NULL, NULL, $3, $4, $5, $6, $7, 'pending', 0,
                      NULL, NULL, NULL, $8, $8, NULL)
            ON CONFLICT (dedup_key) DO UPDATE SET dedup_key = excluded.dedup_key
            RETURNING id, dedup_key, delivery_runtime_session_id, turn_id, terminal_event_seq,
                      effect_kind, payload_json, status,
                      claim_token
            "#,
        )
        .bind(id.to_string())
        .bind(&effect.dedup_key)
        .bind(effect.presentation_thread_id.as_str())
        .bind(effect.presentation_turn_id.as_str())
        .bind(terminal_event_sequence)
        .bind(effect.effect_kind.as_str())
        .bind(&effect.payload)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        let record = control_effect_from_row(&row)?;
        if record.effect_kind != effect.effect_kind
            || record.presentation_thread_id != effect.presentation_thread_id
            || record.presentation_turn_id != effect.presentation_turn_id
            || record.terminal_event_sequence != effect.terminal_event_sequence
            || record.payload != effect.payload
        {
            return Err(format!(
                "control effect dedup key {} was reused with different immutable evidence",
                effect.dedup_key
            ));
        }
        Ok(record)
    }

    async fn claim(
        &self,
        dedup_key: &str,
        owner: &str,
        lease_duration_ms: i64,
    ) -> Result<Option<AgentRunControlEffectRecord>, String> {
        if lease_duration_ms <= 0 || owner.trim().is_empty() {
            return Err(
                "control effect claim requires a non-empty owner and positive lease".into(),
            );
        }
        let now = chrono::Utc::now().timestamp_millis();
        let claim_token = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            UPDATE agent_run_control_effects
            SET status = 'running', attempt_count = attempt_count + 1, claim_token = $2,
                claim_owner = $3, claim_expires_at_ms = $4, updated_at_ms = $1,
                last_error = NULL
            WHERE dedup_key = $5
              AND (status IN ('pending', 'failed')
                   OR (status = 'running'
                       AND (claim_expires_at_ms IS NULL OR claim_expires_at_ms <= $1)))
            RETURNING id, dedup_key, delivery_runtime_session_id, turn_id, terminal_event_seq,
                      effect_kind, payload_json, status,
                      claim_token
            "#,
        )
        .bind(now)
        .bind(claim_token.to_string())
        .bind(owner)
        .bind(now.saturating_add(lease_duration_ms))
        .bind(dedup_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        row.as_ref().map(control_effect_from_row).transpose()
    }

    async fn mark_succeeded(&self, effect_id: Uuid, claim_token: Uuid) -> Result<(), String> {
        finish_claim(&self.pool, effect_id, claim_token, "succeeded", None).await
    }

    async fn mark_failed(
        &self,
        effect_id: Uuid,
        claim_token: Uuid,
        error: String,
    ) -> Result<(), String> {
        finish_claim(&self.pool, effect_id, claim_token, "failed", Some(error)).await
    }
}

fn control_effect_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AgentRunControlEffectRecord, String> {
    let id = Uuid::parse_str(
        row.try_get::<String, _>("id")
            .map_err(|e| e.to_string())?
            .as_str(),
    )
    .map_err(|error| error.to_string())?;
    let kind = row
        .try_get::<String, _>("effect_kind")
        .map_err(|e| e.to_string())?;
    let status = match row
        .try_get::<String, _>("status")
        .map_err(|e| e.to_string())?
        .as_str()
    {
        "pending" => AgentRunControlEffectStatus::Pending,
        "running" => AgentRunControlEffectStatus::Running,
        "succeeded" => AgentRunControlEffectStatus::Succeeded,
        "failed" => AgentRunControlEffectStatus::Failed,
        other => return Err(format!("unknown AgentRun control effect status: {other}")),
    };
    let claim_token = row
        .try_get::<Option<String>, _>("claim_token")
        .map_err(|e| e.to_string())?
        .map(|token| Uuid::parse_str(&token).map_err(|error| error.to_string()))
        .transpose()?;
    Ok(AgentRunControlEffectRecord {
        id,
        dedup_key: row.try_get("dedup_key").map_err(|e| e.to_string())?,
        presentation_thread_id: row
            .try_get::<String, _>("delivery_runtime_session_id")
            .map_err(|e| e.to_string())?
            .parse::<agentdash_agent_runtime_contract::PresentationThreadId>()
            .map_err(|error| error.to_string())?,
        presentation_turn_id: row
            .try_get::<String, _>("turn_id")
            .map_err(|e| e.to_string())?
            .parse::<agentdash_agent_runtime_contract::PresentationTurnId>()
            .map_err(|error| error.to_string())?,
        terminal_event_sequence: agentdash_agent_runtime_contract::EventSequence(
            u64::try_from(
                row.try_get::<i64, _>("terminal_event_seq")
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|_| "negative terminal event sequence".to_string())?,
        ),
        effect_kind: AgentRunControlEffectKind::try_from(kind.as_str())?,
        payload: row.try_get("payload_json").map_err(|e| e.to_string())?,
        status,
        claim_token,
    })
}

async fn finish_claim(
    pool: &PgPool,
    effect_id: Uuid,
    claim_token: Uuid,
    status: &str,
    error: Option<String>,
) -> Result<(), String> {
    let result = sqlx::query(
        r#"
        UPDATE agent_run_control_effects
        SET status = $3, claim_token = NULL, claim_owner = NULL, claim_expires_at_ms = NULL,
            updated_at_ms = $4, last_error = $5
        WHERE id = $1 AND claim_token = $2 AND status = 'running'
        "#,
    )
    .bind(effect_id.to_string())
    .bind(claim_token.to_string())
    .bind(status)
    .bind(chrono::Utc::now().timestamp_millis())
    .bind(error)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    if result.rows_affected() != 1 {
        return Err(format!(
            "control effect {effect_id} is not owned by claim {claim_token}"
        ));
    }
    Ok(())
}
