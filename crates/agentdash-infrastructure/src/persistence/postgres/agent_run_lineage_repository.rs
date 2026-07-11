use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{AgentRunLineage, AgentRunLineageRepository};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::json_document::{from_optional_jsonb, to_optional_jsonb};
use super::sql_err_for;

pub struct PostgresAgentRunLineageRepository {
    pool: PgPool,
}

impl PostgresAgentRunLineageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AgentRunLineageRepository for PostgresAgentRunLineageRepository {
    async fn create(&self, lineage: &AgentRunLineage) -> Result<(), DomainError> {
        insert_agent_run_lineage(&self.pool, lineage).await
    }

    async fn find_parent(
        &self,
        child_run_id: Uuid,
        child_agent_id: Uuid,
    ) -> Result<Option<AgentRunLineage>, DomainError> {
        sqlx::query_as::<_, AgentRunLineageRow>(&format!(
            "SELECT {AGENT_RUN_LINEAGE_COLS} FROM agent_run_lineages \
             WHERE child_run_id=$1 AND child_agent_id=$2"
        ))
        .bind(child_run_id.to_string())
        .bind(child_agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_lineages", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_children(
        &self,
        parent_run_id: Uuid,
        parent_agent_id: Uuid,
    ) -> Result<Vec<AgentRunLineage>, DomainError> {
        sqlx::query_as::<_, AgentRunLineageRow>(&format!(
            "SELECT {AGENT_RUN_LINEAGE_COLS} FROM agent_run_lineages \
             WHERE parent_run_id=$1 AND parent_agent_id=$2 ORDER BY created_at DESC"
        ))
        .bind(parent_run_id.to_string())
        .bind(parent_agent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_lineages", error))?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentRunLineage>, DomainError> {
        sqlx::query_as::<_, AgentRunLineageRow>(&format!(
            "SELECT {AGENT_RUN_LINEAGE_COLS} FROM agent_run_lineages \
             WHERE parent_run_id=$1 OR child_run_id=$1 ORDER BY created_at DESC"
        ))
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_lineages", error))?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }
}

async fn insert_agent_run_lineage(
    pool: &PgPool,
    lineage: &AgentRunLineage,
) -> Result<(), DomainError> {
    sqlx::query(agent_run_lineage_insert_sql())
        .bind(lineage.id.to_string())
        .bind(lineage.parent_run_id.to_string())
        .bind(lineage.parent_agent_id.to_string())
        .bind(lineage.child_run_id.to_string())
        .bind(lineage.child_agent_id.to_string())
        .bind(&lineage.relation_kind)
        .bind(lineage.parent_frame_id.map(|id| id.to_string()))
        .bind(lineage.parent_frame_revision)
        .bind(lineage.child_frame_id.map(|id| id.to_string()))
        .bind(lineage.child_frame_revision)
        .bind(option_u64_to_i64(lineage.fork_point_event_seq)?)
        .bind(to_optional_jsonb(
            lineage.fork_point_ref_json.as_ref(),
            "agent_run_lineages.fork_point_ref",
        )?)
        .bind(&lineage.forked_by_user_id)
        .bind(to_optional_jsonb(
            lineage.metadata_json.as_ref(),
            "agent_run_lineages.metadata",
        )?)
        .bind(lineage.created_at)
        .execute(pool)
        .await
        .map_err(|error| sql_err_for("agent_run_lineages", error))?;
    Ok(())
}

fn agent_run_lineage_insert_sql() -> &'static str {
    r#"INSERT INTO agent_run_lineages
        (id,parent_run_id,parent_agent_id,child_run_id,child_agent_id,relation_kind,
         parent_frame_id,parent_frame_revision,child_frame_id,child_frame_revision,
         fork_point_event_seq,fork_point_ref,forked_by_user_id,metadata,created_at)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)"#
}

const AGENT_RUN_LINEAGE_COLS: &str = "id,parent_run_id,parent_agent_id,child_run_id,child_agent_id,relation_kind,parent_frame_id,parent_frame_revision,child_frame_id,child_frame_revision,fork_point_event_seq,fork_point_ref,forked_by_user_id,metadata,created_at";

#[derive(sqlx::FromRow)]
struct AgentRunLineageRow {
    id: String,
    parent_run_id: String,
    parent_agent_id: String,
    child_run_id: String,
    child_agent_id: String,
    relation_kind: String,
    parent_frame_id: Option<String>,
    parent_frame_revision: Option<i32>,
    child_frame_id: Option<String>,
    child_frame_revision: Option<i32>,
    fork_point_event_seq: Option<i64>,
    fork_point_ref: Option<Value>,
    forked_by_user_id: String,
    metadata: Option<Value>,
    created_at: DateTime<Utc>,
}

impl TryFrom<AgentRunLineageRow> for AgentRunLineage {
    type Error = DomainError;

    fn try_from(row: AgentRunLineageRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(&row.id, "agent_run_lineages.id")?,
            parent_run_id: parse_uuid(&row.parent_run_id, "agent_run_lineages.parent_run_id")?,
            parent_agent_id: parse_uuid(
                &row.parent_agent_id,
                "agent_run_lineages.parent_agent_id",
            )?,
            child_run_id: parse_uuid(&row.child_run_id, "agent_run_lineages.child_run_id")?,
            child_agent_id: parse_uuid(&row.child_agent_id, "agent_run_lineages.child_agent_id")?,
            relation_kind: row.relation_kind,
            parent_frame_id: opt_uuid(
                row.parent_frame_id.as_ref(),
                "agent_run_lineages.parent_frame_id",
            )?,
            parent_frame_revision: row.parent_frame_revision,
            child_frame_id: opt_uuid(
                row.child_frame_id.as_ref(),
                "agent_run_lineages.child_frame_id",
            )?,
            child_frame_revision: row.child_frame_revision,
            fork_point_event_seq: option_i64_to_u64(row.fork_point_event_seq)?,
            fork_point_ref_json: from_optional_jsonb(
                row.fork_point_ref,
                "agent_run_lineages.fork_point_ref",
            )?,
            forked_by_user_id: row.forked_by_user_id,
            metadata_json: from_optional_jsonb(row.metadata, "agent_run_lineages.metadata")?,
            created_at: row.created_at,
        })
    }
}

fn parse_uuid(raw: &str, ctx: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{ctx} invalid uuid `{raw}`: {error}")))
}

fn opt_uuid(raw: Option<&String>, ctx: &str) -> Result<Option<Uuid>, DomainError> {
    raw.map(|value| parse_uuid(value, ctx)).transpose()
}

fn option_u64_to_i64(value: Option<u64>) -> Result<Option<i64>, DomainError> {
    value
        .map(|value| {
            i64::try_from(value).map_err(|_| {
                DomainError::InvalidConfig(format!(
                    "agent_run_lineages.fork_point_event_seq 超出 bigint 范围: {value}"
                ))
            })
        })
        .transpose()
}

fn option_i64_to_u64(value: Option<i64>) -> Result<Option<u64>, DomainError> {
    value
        .map(|value| {
            u64::try_from(value).map_err(|_| {
                DomainError::InvalidConfig(format!(
                    "agent_run_lineages.fork_point_event_seq 不能为负数: {value}"
                ))
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_run_lineage_row_maps_json_and_refs() {
        let parent_run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let parent_frame_id = Uuid::new_v4();
        let child_run_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let created_at = Utc::now();

        let lineage = AgentRunLineage::try_from(AgentRunLineageRow {
            id: Uuid::new_v4().to_string(),
            parent_run_id: parent_run_id.to_string(),
            parent_agent_id: parent_agent_id.to_string(),
            child_run_id: child_run_id.to_string(),
            child_agent_id: child_agent_id.to_string(),
            relation_kind: "fork".to_string(),
            parent_frame_id: Some(parent_frame_id.to_string()),
            parent_frame_revision: Some(7),
            child_frame_id: Some(child_frame_id.to_string()),
            child_frame_revision: Some(1),
            fork_point_event_seq: Some(42),
            fork_point_ref: Some(json!({ "turn_id": "turn-1", "entry_index": 3 })),
            forked_by_user_id: "user-child".to_string(),
            metadata: Some(json!({ "reason": "explore" })),
            created_at,
        })
        .expect("lineage row should map");

        assert_eq!(lineage.parent_run_id, parent_run_id);
        assert_eq!(lineage.parent_agent_id, parent_agent_id);
        assert_eq!(lineage.child_run_id, child_run_id);
        assert_eq!(lineage.child_agent_id, child_agent_id);
        assert_eq!(lineage.relation_kind, "fork");
        assert_eq!(lineage.parent_frame_id, Some(parent_frame_id));
        assert_eq!(lineage.parent_frame_revision, Some(7));
        assert_eq!(lineage.child_frame_id, Some(child_frame_id));
        assert_eq!(lineage.child_frame_revision, Some(1));
        assert_eq!(lineage.fork_point_event_seq, Some(42));
        assert_eq!(
            lineage.fork_point_ref_json,
            Some(json!({ "turn_id": "turn-1", "entry_index": 3 }))
        );
        assert_eq!(lineage.forked_by_user_id, "user-child");
        assert_eq!(lineage.metadata_json, Some(json!({ "reason": "explore" })));
        assert_eq!(lineage.created_at, created_at);
    }

    #[test]
    fn agent_run_lineage_row_rejects_negative_fork_point_event_seq() {
        let row = AgentRunLineageRow {
            id: Uuid::new_v4().to_string(),
            parent_run_id: Uuid::new_v4().to_string(),
            parent_agent_id: Uuid::new_v4().to_string(),
            child_run_id: Uuid::new_v4().to_string(),
            child_agent_id: Uuid::new_v4().to_string(),
            relation_kind: "fork".to_string(),
            parent_frame_id: None,
            parent_frame_revision: None,
            child_frame_id: None,
            child_frame_revision: None,
            fork_point_event_seq: Some(-1),
            fork_point_ref: None,
            forked_by_user_id: "user-child".to_string(),
            metadata: None,
            created_at: Utc::now(),
        };

        let error = AgentRunLineage::try_from(row).expect_err("negative seq should fail");
        assert!(error.to_string().contains("fork_point_event_seq"));
    }
}
