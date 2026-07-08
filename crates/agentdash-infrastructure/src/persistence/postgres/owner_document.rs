use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::DomainError;

use super::db_err;

pub(crate) async fn mutate_typed_jsonb_owner_document<T, F>(
    pool: &PgPool,
    owner_id: Uuid,
    document_field: &'static str,
    lock_sql: &'static str,
    update_sql: &'static str,
    mutate: F,
) -> Result<T, DomainError>
where
    T: DeserializeOwned + Serialize,
    F: FnOnce(&mut T) -> Result<(), DomainError>,
{
    let mut tx = pool.begin().await.map_err(db_err)?;
    let Some(raw_document) = sqlx::query_scalar::<_, Value>(lock_sql)
        .bind(owner_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?
    else {
        return Err(DomainError::NotFound {
            entity: "owner_document",
            id: owner_id.to_string(),
        });
    };

    let mut document: T = serde_json::from_value(raw_document)
        .map_err(|error| DomainError::InvalidConfig(format!("{document_field}: {error}")))?;
    mutate(&mut document)?;
    let document_json = serde_json::to_value(&document).map_err(DomainError::Serialization)?;

    sqlx::query(update_sql)
        .bind(document_json)
        .bind(chrono::Utc::now())
        .bind(owner_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

    tx.commit().await.map_err(db_err)?;
    Ok(document)
}
