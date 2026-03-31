#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSession {
    pub token_hash: String,
    pub identity_json: String,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}
