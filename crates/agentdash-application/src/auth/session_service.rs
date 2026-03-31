use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use agentdash_domain::auth_session::{AuthSession, AuthSessionRepository};
use agentdash_spi::auth::AuthIdentity;
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub struct AuthSessionService {
    repo: Arc<dyn AuthSessionRepository>,
}

#[derive(Debug, Error)]
pub enum AuthSessionServiceError {
    #[error("认证会话存储失败: {0}")]
    Storage(String),
    #[error("认证身份序列化失败: {0}")]
    Serialize(String),
    #[error("认证身份反序列化失败: {0}")]
    Deserialize(String),
}

#[derive(Debug, Deserialize)]
struct JwtClaims {
    #[serde(default)]
    exp: Option<u64>,
}

impl AuthSessionService {
    pub fn new(repo: Arc<dyn AuthSessionRepository>) -> Self {
        Self { repo }
    }

    pub async fn save_login_session(
        &self,
        token: &str,
        identity: &AuthIdentity,
    ) -> Result<(), AuthSessionServiceError> {
        let now = now_epoch_secs();
        let session = AuthSession {
            token_hash: hash_token(token),
            identity_json: serde_json::to_string(identity)
                .map_err(|e| AuthSessionServiceError::Serialize(e.to_string()))?,
            expires_at: extract_jwt_exp(token).and_then(|v| i64::try_from(v).ok()),
            revoked_at: None,
            created_at: now,
            updated_at: now,
        };
        self.repo
            .upsert_session(&session)
            .await
            .map_err(|e| AuthSessionServiceError::Storage(e.to_string()))
    }

    pub async fn resolve_identity_by_token(
        &self,
        token: &str,
    ) -> Result<Option<AuthIdentity>, AuthSessionServiceError> {
        let session = self
            .repo
            .get_by_token_hash(&hash_token(token))
            .await
            .map_err(|e| AuthSessionServiceError::Storage(e.to_string()))?;

        let Some(session) = session else {
            return Ok(None);
        };
        if session.revoked_at.is_some() {
            return Ok(None);
        }
        if session
            .expires_at
            .is_some_and(|exp| exp <= now_epoch_secs())
        {
            return Ok(None);
        }

        let identity: AuthIdentity = serde_json::from_str(&session.identity_json)
            .map_err(|e| AuthSessionServiceError::Deserialize(e.to_string()))?;
        Ok(Some(identity))
    }

    pub async fn revoke_token(&self, token: &str) -> Result<bool, AuthSessionServiceError> {
        let now = now_epoch_secs();
        self.repo
            .revoke_by_token_hash(&hash_token(token), now)
            .await
            .map_err(|e| AuthSessionServiceError::Storage(e.to_string()))
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64, AuthSessionServiceError> {
        let now = now_epoch_secs();
        self.repo
            .delete_expired_before(now)
            .await
            .map_err(|e| AuthSessionServiceError::Storage(e.to_string()))
    }
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn extract_jwt_exp(token: &str) -> Option<u64> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let payload = parts[1];
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(payload))
        .ok()?;
    let claims: JwtClaims = serde_json::from_slice(&payload_bytes).ok()?;
    claims.exp
}
