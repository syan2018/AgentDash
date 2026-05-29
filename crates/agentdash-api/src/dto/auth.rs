use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct TokenQuery {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RevokeTokenRequest {
    pub access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct OidcCallbackQuery {
    pub code: String,
    pub state: String,
}
