use sqlx::PgPool;

use agentdash_domain::backend::{
    BackendConfig, BackendRepository, BackendShareScopeKind, BackendType, BackendVisibility,
    LocalBackendClaim, UserPreferences, ViewConfig,
};
use agentdash_domain::common::error::DomainError;

pub struct PostgresBackendRepository {
    pool: PgPool,
}

impl PostgresBackendRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["backends", "views", "user_preferences"],
        )
        .await
    }
}

#[async_trait::async_trait]
impl BackendRepository for PostgresBackendRepository {
    async fn add_backend(&self, config: &BackendConfig) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO backends (
                id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                profile_id, device_id, machine_id, machine_label, legacy_machine_ids,
                visibility, share_scope_kind, share_scope_id, capability_slot, device, last_claimed_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               endpoint = excluded.endpoint,
               auth_token = excluded.auth_token,
               enabled = excluded.enabled,
               backend_type = excluded.backend_type,
               owner_user_id = excluded.owner_user_id,
               profile_id = excluded.profile_id,
               device_id = excluded.device_id,
               machine_id = excluded.machine_id,
               machine_label = excluded.machine_label,
               legacy_machine_ids = excluded.legacy_machine_ids,
               visibility = excluded.visibility,
               share_scope_kind = excluded.share_scope_kind,
               share_scope_id = excluded.share_scope_id,
               capability_slot = excluded.capability_slot,
               device = excluded.device,
               last_claimed_at = excluded.last_claimed_at",
        )
        .bind(&config.id)
        .bind(&config.name)
        .bind(&config.endpoint)
        .bind(&config.auth_token)
        .bind(config.enabled)
        .bind(serde_json::to_string(&config.backend_type)?.trim_matches('"'))
        .bind(&config.owner_user_id)
        .bind(&config.profile_id)
        .bind(&config.device_id)
        .bind(&config.machine_id)
        .bind(&config.machine_label)
        .bind(serde_json::json!(&config.legacy_machine_ids))
        .bind(config.visibility.as_str())
        .bind(config.share_scope_kind.as_str())
        .bind(&config.share_scope_id)
        .bind(&config.capability_slot)
        .bind(&config.device)
        .bind(config.last_claimed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn list_backends(&self) -> Result<Vec<BackendConfig>, DomainError> {
        let rows = sqlx::query_as::<_, BackendRow>(
            "SELECT id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                    profile_id, device_id, machine_id, machine_label,
                    COALESCE(legacy_machine_ids, '[]'::jsonb) AS legacy_machine_ids,
                    visibility, share_scope_kind, share_scope_id, capability_slot,
                    COALESCE(device, '{}'::jsonb) AS device, last_claimed_at
             FROM backends ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn get_backend(&self, id: &str) -> Result<BackendConfig, DomainError> {
        let row = sqlx::query_as::<_, BackendRow>(
            "SELECT id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                    profile_id, device_id, machine_id, machine_label,
                    COALESCE(legacy_machine_ids, '[]'::jsonb) AS legacy_machine_ids,
                    visibility, share_scope_kind, share_scope_id, capability_slot,
                    COALESCE(device, '{}'::jsonb) AS device, last_claimed_at
             FROM backends WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "backend",
            id: id.to_string(),
        })?;

        row.try_into()
    }

    async fn get_backend_by_auth_token(&self, token: &str) -> Result<BackendConfig, DomainError> {
        let rows = sqlx::query_as::<_, BackendRow>(
            "SELECT id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                    profile_id, device_id, machine_id, machine_label,
                    COALESCE(legacy_machine_ids, '[]'::jsonb) AS legacy_machine_ids,
                    visibility, share_scope_kind, share_scope_id, capability_slot,
                    COALESCE(device, '{}'::jsonb) AS device, last_claimed_at
             FROM backends WHERE auth_token = $1",
        )
        .bind(token)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        match rows.len() {
            0 => Err(DomainError::NotFound {
                entity: "backend_auth_token",
                id: token.to_string(),
            }),
            1 => Ok(rows
                .into_iter()
                .next()
                .expect("rows.len() == 1 时必须存在")
                .try_into()?),
            _ => Err(DomainError::InvalidConfig(
                "检测到重复 backend auth_token 配置".to_string(),
            )),
        }
    }

    async fn ensure_local_backend(
        &self,
        claim: &LocalBackendClaim,
    ) -> Result<BackendConfig, DomainError> {
        let identity_candidates = local_claim_identity_candidates(claim);
        let candidate_rows = sqlx::query_as::<_, (String,)>(
            r#"
            WITH claim_ids AS (
                SELECT lower(value) AS value
                  FROM unnest($5::text[]) AS ids(value)
            )
            SELECT id
              FROM backends AS b
             WHERE b.backend_type = 'local'
               AND b.share_scope_kind = $2
               AND (
                    COALESCE(b.share_scope_id, '') = COALESCE($3, '')
                    OR (
                        COALESCE(b.share_scope_id, '') = ''
                        AND b.owner_user_id IS NULL
                    )
               )
               AND capability_slot = $4
                AND (
                     b.machine_id = $1
                     OR lower(COALESCE(b.machine_id, '')) IN (SELECT value FROM claim_ids)
                     OR lower(COALESCE(b.device_id, '')) IN (SELECT value FROM claim_ids)
                     OR EXISTS (
                         SELECT 1
                           FROM jsonb_array_elements_text(
                              COALESCE(b.legacy_machine_ids, '[]'::jsonb)
                          ) AS legacy(value)
                         WHERE lower(legacy.value) IN (SELECT value FROM claim_ids)
                    )
               )
             ORDER BY CASE
                    WHEN machine_id = $1 THEN 0
                    WHEN lower(COALESCE(machine_label, '')) = lower($7) THEN 1
                    WHEN id = $6 THEN 2
                    ELSE 3
               END,
               last_claimed_at DESC NULLS LAST,
               created_at DESC,
               id ASC
            "#,
        )
        .bind(&claim.machine_id)
        .bind(claim.share_scope_kind.as_str())
        .bind(&claim.share_scope_id)
        .bind(&claim.capability_slot)
        .bind(&identity_candidates)
        .bind(&claim.backend_id)
        .bind(&claim.machine_label)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let mut candidate_ids: Vec<String> = candidate_rows.into_iter().map(|(id,)| id).collect();
        candidate_ids.dedup();
        let existing_id = candidate_ids.first().cloned();

        let row = if let Some(existing_id) = existing_id {
            let duplicate_ids: Vec<String> = candidate_ids
                .into_iter()
                .filter(|id| id != &existing_id)
                .collect();
            if !duplicate_ids.is_empty() {
                merge_duplicate_local_backend_rows(&self.pool, &existing_id, &duplicate_ids)
                    .await?;
            }

            sqlx::query_as::<_, BackendRow>(
                r#"
                UPDATE backends
                   SET name = $2,
                       endpoint = $3,
                       auth_token = CASE
                           WHEN $4 THEN $5
                           ELSE COALESCE(auth_token, $5)
                       END,
                       enabled = TRUE,
                       backend_type = 'local',
                       owner_user_id = $6,
                       profile_id = $7,
                       machine_id = $8,
                       machine_label = $9,
                       legacy_machine_ids = (
                           SELECT COALESCE(jsonb_agg(value), '[]'::jsonb)
                             FROM (
                                 SELECT DISTINCT value
                                   FROM (
                                       SELECT value
                                         FROM jsonb_array_elements_text(
                                             COALESCE(backends.legacy_machine_ids, '[]'::jsonb)
                                         ) AS ids(value)
                                        UNION ALL SELECT backends.machine_id
                                        UNION ALL SELECT backends.device_id
                                        UNION ALL SELECT value
                                          FROM jsonb_array_elements_text($10) AS ids(value)
                                   ) raw(value)
                                  WHERE value IS NOT NULL
                                    AND btrim(value) <> ''
                                    AND value <> $8
                             ) merged
                       ),
                       visibility = $11,
                       share_scope_kind = $12,
                       share_scope_id = $13,
                       capability_slot = $14,
                       device = $15,
                       last_claimed_at = now()
                 WHERE id = $1
                 RETURNING id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                           profile_id, device_id, machine_id, machine_label,
                           COALESCE(legacy_machine_ids, '[]'::jsonb) AS legacy_machine_ids,
                           visibility, share_scope_kind, share_scope_id, capability_slot,
                           COALESCE(device, '{}'::jsonb) AS device, last_claimed_at
                "#,
            )
            .bind(existing_id)
            .bind(&claim.name)
            .bind(&claim.endpoint)
            .bind(claim.rotate_token)
            .bind(&claim.auth_token)
            .bind(&claim.owner_user_id)
            .bind(&claim.profile_id)
            .bind(&claim.machine_id)
            .bind(&claim.machine_label)
            .bind(serde_json::json!(&claim.legacy_machine_ids))
            .bind(claim.visibility.as_str())
            .bind(claim.share_scope_kind.as_str())
            .bind(&claim.share_scope_id)
            .bind(&claim.capability_slot)
            .bind(&claim.device)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?
        } else {
            sqlx::query_as::<_, BackendRow>(
                r#"
                INSERT INTO backends (
                    id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                    profile_id, machine_id, machine_label, legacy_machine_ids,
                    visibility, share_scope_kind, share_scope_id, capability_slot, device, last_claimed_at
                )
                VALUES ($1, $2, $3, $4, TRUE, 'local', $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, now())
                RETURNING id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                          profile_id, device_id, machine_id, machine_label,
                          COALESCE(legacy_machine_ids, '[]'::jsonb) AS legacy_machine_ids,
                          visibility, share_scope_kind, share_scope_id, capability_slot,
                          COALESCE(device, '{}'::jsonb) AS device, last_claimed_at
                "#,
            )
            .bind(&claim.backend_id)
            .bind(&claim.name)
            .bind(&claim.endpoint)
            .bind(&claim.auth_token)
            .bind(&claim.owner_user_id)
            .bind(&claim.profile_id)
            .bind(&claim.machine_id)
            .bind(&claim.machine_label)
            .bind(serde_json::json!(&claim.legacy_machine_ids))
            .bind(claim.visibility.as_str())
            .bind(claim.share_scope_kind.as_str())
            .bind(&claim.share_scope_id)
            .bind(&claim.capability_slot)
            .bind(&claim.device)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?
        };

        row.try_into()
    }

    async fn remove_backend(&self, id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM backends WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn list_views(&self) -> Result<Vec<ViewConfig>, DomainError> {
        let rows = sqlx::query_as::<_, ViewRow>(
            "SELECT id, name, backend_ids, filters, sort_by FROM views ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn save_view(&self, view: &ViewConfig) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO views (id, name, backend_ids, filters, sort_by)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                backend_ids = EXCLUDED.backend_ids,
                filters = EXCLUDED.filters,
                sort_by = EXCLUDED.sort_by",
        )
        .bind(&view.id)
        .bind(&view.name)
        .bind(serde_json::to_string(&view.backend_ids)?)
        .bind(view.filters.to_string())
        .bind(&view.sort_by)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn get_preferences(&self) -> Result<UserPreferences, DomainError> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT value FROM user_preferences WHERE key = 'prefs'",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        match row {
            Some((json,)) => Ok(serde_json::from_str(&json)?),
            None => Ok(UserPreferences::default()),
        }
    }

    async fn save_preferences(&self, prefs: &UserPreferences) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO user_preferences (key, value) VALUES ('prefs', $1)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
        )
        .bind(serde_json::to_string(prefs)?)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

// --- SQLx 行映射 ---

#[derive(sqlx::FromRow)]
struct BackendRow {
    id: String,
    name: String,
    endpoint: String,
    auth_token: Option<String>,
    enabled: bool,
    backend_type: String,
    owner_user_id: Option<String>,
    profile_id: Option<String>,
    device_id: Option<String>,
    machine_id: Option<String>,
    machine_label: Option<String>,
    legacy_machine_ids: serde_json::Value,
    visibility: String,
    share_scope_kind: String,
    share_scope_id: Option<String>,
    capability_slot: String,
    device: serde_json::Value,
    last_claimed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl TryFrom<BackendRow> for BackendConfig {
    type Error = DomainError;

    fn try_from(row: BackendRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            name: row.name,
            endpoint: row.endpoint,
            auth_token: row.auth_token,
            enabled: row.enabled,
            backend_type: parse_backend_type(&row.backend_type)?,
            owner_user_id: row.owner_user_id,
            profile_id: row.profile_id,
            device_id: row.device_id,
            machine_id: row.machine_id,
            machine_label: row.machine_label,
            legacy_machine_ids: serde_json::from_value(row.legacy_machine_ids)?,
            visibility: parse_backend_visibility(&row.visibility)?,
            share_scope_kind: parse_backend_share_scope_kind(&row.share_scope_kind)?,
            share_scope_id: row.share_scope_id,
            capability_slot: row.capability_slot,
            device: row.device,
            last_claimed_at: row.last_claimed_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct ViewRow {
    id: String,
    name: String,
    backend_ids: String,
    filters: String,
    sort_by: Option<String>,
}

impl TryFrom<ViewRow> for ViewConfig {
    type Error = DomainError;

    fn try_from(row: ViewRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            name: row.name,
            backend_ids: parse_json_column(&row.backend_ids, "views.backend_ids")?,
            filters: parse_json_column(&row.filters, "views.filters")?,
            sort_by: row.sort_by,
        })
    }
}

fn parse_backend_type(raw: &str) -> Result<BackendType, DomainError> {
    match raw {
        "local" => Ok(BackendType::Local),
        "remote" => Ok(BackendType::Remote),
        _ => Err(DomainError::InvalidConfig(format!(
            "backends.backend_type: 未知值 `{raw}`"
        ))),
    }
}

fn parse_backend_visibility(raw: &str) -> Result<BackendVisibility, DomainError> {
    match raw {
        "private" => Ok(BackendVisibility::Private),
        "shared" => Ok(BackendVisibility::Shared),
        "system" => Ok(BackendVisibility::System),
        _ => Err(DomainError::InvalidConfig(format!(
            "backends.visibility: 未知值 `{raw}`"
        ))),
    }
}

fn parse_backend_share_scope_kind(raw: &str) -> Result<BackendShareScopeKind, DomainError> {
    match raw {
        "user" => Ok(BackendShareScopeKind::User),
        "project" => Ok(BackendShareScopeKind::Project),
        "system" => Ok(BackendShareScopeKind::System),
        _ => Err(DomainError::InvalidConfig(format!(
            "backends.share_scope_kind: 未知值 `{raw}`"
        ))),
    }
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    field: &str,
) -> Result<T, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn local_claim_identity_candidates(claim: &LocalBackendClaim) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    std::iter::once(claim.machine_id.as_str())
        .chain(claim.legacy_machine_ids.iter().map(String::as_str))
        .flat_map(identity_aliases)
        .filter(|value| seen.insert(value.to_ascii_lowercase()))
        .collect()
}

fn identity_aliases(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let lower = trimmed.to_ascii_lowercase();
    let mut aliases = vec![trimmed.to_string(), lower.clone()];
    if !lower.ends_with(".local") {
        aliases.push(format!("{lower}.local"));
    }
    aliases
}

async fn merge_duplicate_local_backend_rows(
    pool: &PgPool,
    canonical_id: &str,
    duplicate_ids: &[String],
) -> Result<(), DomainError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
    let workspace_bindings_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('workspace_bindings')::text")
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

    if workspace_bindings_table.is_some() {
        sqlx::query("UPDATE workspace_bindings SET backend_id = $1 WHERE backend_id = ANY($2)")
            .bind(canonical_id)
            .bind(duplicate_ids)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
    }

    sqlx::query(
        r#"
        UPDATE views
           SET backend_ids = (
               SELECT COALESCE(jsonb_agg(id ORDER BY first_seen)::text, '[]')
                 FROM (
                     SELECT id, MIN(ord) AS first_seen
                       FROM (
                           SELECT CASE
                                      WHEN value = ANY($2) THEN $1
                                      ELSE value
                                  END AS id,
                                  ord
                             FROM jsonb_array_elements_text(backend_ids::jsonb)
                                  WITH ORDINALITY AS items(value, ord)
                       ) replaced
                      WHERE btrim(id) <> ''
                      GROUP BY id
                 ) deduped
           )
         WHERE EXISTS (
             SELECT 1
               FROM jsonb_array_elements_text(backend_ids::jsonb) AS items(value)
              WHERE value = ANY($2)
         )
        "#,
    )
    .bind(canonical_id)
    .bind(duplicate_ids)
    .execute(&mut *tx)
    .await
    .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

    sqlx::query("DELETE FROM backends WHERE id = ANY($1)")
        .bind(duplicate_ids)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

    tx.commit()
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::postgres::test_pg_pool;

    fn backend(id: &str, token: Option<&str>) -> BackendConfig {
        BackendConfig {
            id: id.to_string(),
            name: format!("backend-{id}"),
            endpoint: "ws://localhost".to_string(),
            auth_token: token.map(str::to_string),
            enabled: true,
            backend_type: BackendType::Local,
            owner_user_id: None,
            profile_id: None,
            device_id: None,
            machine_id: None,
            machine_label: None,
            legacy_machine_ids: Vec::new(),
            visibility: BackendVisibility::Private,
            share_scope_kind: BackendShareScopeKind::User,
            share_scope_id: None,
            capability_slot: "default".to_string(),
            device: serde_json::json!({}),
            last_claimed_at: None,
        }
    }

    fn local_claim() -> LocalBackendClaim {
        LocalBackendClaim {
            owner_user_id: "user-a".to_string(),
            profile_id: "desktop-local".to_string(),
            machine_id: "machine-a".to_string(),
            machine_label: "Desktop A".to_string(),
            legacy_machine_ids: vec!["device-a".to_string()],
            visibility: BackendVisibility::Private,
            share_scope_kind: BackendShareScopeKind::User,
            share_scope_id: Some("user-a".to_string()),
            capability_slot: "default".to_string(),
            backend_id: "local-first".to_string(),
            name: "Desktop A".to_string(),
            endpoint: "ws://localhost/ws/backend".to_string(),
            auth_token: "token-a".to_string(),
            device: serde_json::json!({ "os": "windows" }),
            rotate_token: false,
        }
    }

    async fn new_repo() -> Option<PostgresBackendRepository> {
        let pool = match test_pg_pool("backend_repository").await {
            Some(pool) => pool,
            None => return None,
        };
        let repo = PostgresBackendRepository::new(pool);
        repo.initialize().await.expect("应能初始化 schema");
        Some(repo)
    }

    #[tokio::test]
    async fn get_backend_by_auth_token_returns_matching_backend() {
        let Some(repo) = new_repo().await else {
            return;
        };
        repo.add_backend(&backend("local-a", Some("secret-a")))
            .await
            .expect("应能插入 backend");

        let found = repo
            .get_backend_by_auth_token("secret-a")
            .await
            .expect("应能按 token 查到 backend");

        assert_eq!(found.id, "local-a");
    }

    #[tokio::test]
    async fn get_backend_by_auth_token_rejects_duplicate_token_binding() {
        let Some(repo) = new_repo().await else {
            return;
        };
        repo.add_backend(&backend("local-a", Some("shared-token")))
            .await
            .expect("应能插入首个 backend");
        repo.add_backend(&backend("local-b", Some("shared-token")))
            .await
            .expect("当前 schema 允许重复 token，用于验证运行时收口");

        let err = repo
            .get_backend_by_auth_token("shared-token")
            .await
            .expect_err("重复 token 绑定应在查询时失败");

        assert!(matches!(err, DomainError::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn add_backend_overwrites_existing_backend_with_same_id() {
        let Some(repo) = new_repo().await else {
            return;
        };
        repo.add_backend(&backend("local-a", Some("secret-a")))
            .await
            .expect("应能插入 backend");

        let mut updated = backend("local-a", Some("secret-b"));
        updated.name = "renamed".to_string();
        repo.add_backend(&updated)
            .await
            .expect("相同 id 应覆盖保存");

        let found = repo
            .get_backend("local-a")
            .await
            .expect("应能取回覆盖后的 backend");

        assert_eq!(found.name, "renamed");
        assert_eq!(found.auth_token.as_deref(), Some("secret-b"));
    }

    #[tokio::test]
    async fn ensure_local_backend_reuses_existing_machine_scope_token() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let claim = local_claim();
        let first = repo
            .ensure_local_backend(&claim)
            .await
            .expect("首次 ensure 应创建 backend");
        assert_eq!(first.id, "local-first");
        assert_eq!(first.auth_token.as_deref(), Some("token-a"));

        let mut second_claim = claim.clone();
        second_claim.backend_id = "local-second".to_string();
        second_claim.auth_token = "token-b".to_string();
        second_claim.name = "Desktop A renamed".to_string();
        let second = repo
            .ensure_local_backend(&second_claim)
            .await
            .expect("同一 machine/scope 应复用 backend");

        assert_eq!(second.id, "local-first");
        assert_eq!(second.name, "Desktop A renamed");
        assert_eq!(second.auth_token.as_deref(), Some("token-a"));
        assert_eq!(second.owner_user_id.as_deref(), Some("user-a"));
        assert_eq!(second.profile_id.as_deref(), Some("desktop-local"));
        assert_eq!(second.machine_id.as_deref(), Some("machine-a"));
        assert_eq!(second.share_scope_id.as_deref(), Some("user-a"));
        assert_eq!(second.legacy_machine_ids, vec!["device-a".to_string()]);
    }

    #[tokio::test]
    async fn ensure_local_backend_can_rotate_token() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let mut claim = local_claim();
        claim.device = serde_json::json!({});
        repo.ensure_local_backend(&claim)
            .await
            .expect("首次 ensure 应创建 backend");

        claim.auth_token = "token-b".to_string();
        claim.rotate_token = true;
        let rotated = repo
            .ensure_local_backend(&claim)
            .await
            .expect("显式 rotate 应替换 token");

        assert_eq!(rotated.auth_token.as_deref(), Some("token-b"));
    }

    #[tokio::test]
    async fn ensure_local_backend_merges_legacy_machine_candidates() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let mut desktop_old = backend("desktop-old", Some("desktop-token"));
        desktop_old.owner_user_id = Some("user-a".to_string());
        desktop_old.profile_id = Some("desktop-local".to_string());
        desktop_old.machine_id = Some("old-desktop-machine".to_string());
        desktop_old.machine_label = Some("LIAOYIHAO-P".to_string());
        desktop_old.share_scope_id = Some("user-a".to_string());
        repo.add_backend(&desktop_old)
            .await
            .expect("应能插入旧桌面端 backend");

        let mut dev_old = backend("dev-old", Some("dev-token"));
        dev_old.owner_user_id = Some("user-a".to_string());
        dev_old.profile_id = Some("dev-joint".to_string());
        dev_old.machine_id = Some("old-dev-machine".to_string());
        dev_old.machine_label = Some("dev-local".to_string());
        dev_old.share_scope_id = Some("user-a".to_string());
        repo.add_backend(&dev_old)
            .await
            .expect("应能插入旧 dev backend");

        let mut orphan_dev = backend("local-dev-1", Some("orphan-token"));
        orphan_dev.device_id = Some("legacy-orphan-device".to_string());
        repo.add_backend(&orphan_dev)
            .await
            .expect("应能插入旧手工 dev backend");

        repo.save_view(&ViewConfig {
            id: "view-local".to_string(),
            name: "本机视图".to_string(),
            backend_ids: vec![
                "dev-old".to_string(),
                "remote-a".to_string(),
                "local-dev-1".to_string(),
                "dev-old".to_string(),
            ],
            filters: serde_json::json!({}),
            sort_by: None,
        })
        .await
        .expect("应能保存引用旧 backend 的视图");

        let mut claim = local_claim();
        claim.machine_id = "shared-dev-machine".to_string();
        claim.machine_label = "liaoyihao-p".to_string();
        claim.legacy_machine_ids = vec![
            "old-desktop-machine".to_string(),
            "old-dev-machine".to_string(),
            "legacy-orphan-device".to_string(),
        ];
        claim.backend_id = "local-new".to_string();
        claim.auth_token = "new-token".to_string();

        let merged = repo
            .ensure_local_backend(&claim)
            .await
            .expect("legacy machine label 应合并旧 backend");

        assert_ne!(merged.id, "local-new");
        assert_eq!(merged.machine_id.as_deref(), Some("shared-dev-machine"));
        assert_eq!(merged.machine_label.as_deref(), Some("liaoyihao-p"));
        assert_eq!(merged.auth_token.as_deref(), Some("desktop-token"));
        assert!(
            merged
                .legacy_machine_ids
                .iter()
                .any(|value| value == "old-desktop-machine")
        );
        assert!(
            repo.get_backend("dev-old").await.is_err(),
            "重复 legacy backend 应被合并清理"
        );
        assert!(
            repo.get_backend("local-dev-1").await.is_err(),
            "旧手工 backend 应被合并清理"
        );
        let views = repo.list_views().await.expect("应能读取视图");
        let view = views
            .into_iter()
            .find(|item| item.id == "view-local")
            .expect("视图应仍然存在");
        assert_eq!(
            view.backend_ids,
            vec!["desktop-old".to_string(), "remote-a".to_string()]
        );
    }

    #[tokio::test]
    async fn ensure_local_backend_does_not_merge_by_machine_label_only() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let mut same_label = backend("same-label", Some("label-token"));
        same_label.owner_user_id = Some("user-a".to_string());
        same_label.profile_id = Some("desktop-local".to_string());
        same_label.machine_id = Some("other-machine".to_string());
        same_label.machine_label = Some("LIAOYIHAO-P".to_string());
        same_label.share_scope_id = Some("user-a".to_string());
        repo.add_backend(&same_label)
            .await
            .expect("应能插入同名机器 backend");

        let mut claim = local_claim();
        claim.machine_id = "current-machine".to_string();
        claim.machine_label = "LIAOYIHAO-P".to_string();
        claim.legacy_machine_ids = Vec::new();
        claim.backend_id = "local-current".to_string();

        let ensured = repo
            .ensure_local_backend(&claim)
            .await
            .expect("同名但不同 machine_id 应创建新 backend");

        assert_eq!(ensured.id, "local-current");
        assert!(
            repo.get_backend("same-label").await.is_ok(),
            "同名机器不应仅因展示标签被合并"
        );
    }
}
