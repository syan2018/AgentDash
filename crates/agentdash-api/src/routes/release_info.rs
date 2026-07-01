use axum::Json;
use serde::Serialize;

const PRODUCT_NAME: &str = "AgentDash";
const DEFAULT_RELAY_PROTOCOL_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct VersionInfoResponse {
    pub version: &'static str,
    pub git_sha: &'static str,
    pub build_time: &'static str,
    pub schema_version: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentDashDiscoveryResponse {
    pub product: &'static str,
    pub public_origin: String,
    pub api_base_url: String,
    pub relay_ws_url: String,
    pub server_version: &'static str,
    pub min_desktop_version: String,
    pub recommended_desktop_version: String,
    pub relay_protocol_version: String,
}

pub async fn version_info() -> Json<VersionInfoResponse> {
    Json(build_version_info())
}

pub async fn agentdash_discovery() -> Json<AgentDashDiscoveryResponse> {
    Json(build_agentdash_discovery())
}

fn build_version_info() -> VersionInfoResponse {
    VersionInfoResponse {
        version: env!("CARGO_PKG_VERSION"),
        git_sha: option_env!("AGENTDASH_GIT_SHA").unwrap_or("unknown"),
        build_time: option_env!("AGENTDASH_BUILD_TIME").unwrap_or("unknown"),
        schema_version: env!("AGENTDASH_SCHEMA_VERSION")
            .parse::<i64>()
            .unwrap_or_default(),
    }
}

fn build_agentdash_discovery() -> AgentDashDiscoveryResponse {
    let public_origin = configured_public_origin();
    let api_base_url = format!("{public_origin}/api");
    let relay_ws_url = derive_relay_ws_url(&public_origin);
    let version = env!("CARGO_PKG_VERSION");

    AgentDashDiscoveryResponse {
        product: PRODUCT_NAME,
        public_origin,
        api_base_url,
        relay_ws_url,
        server_version: version,
        min_desktop_version: runtime_env("AGENTDASH_MIN_DESKTOP_VERSION")
            .unwrap_or_else(|| version.to_string()),
        recommended_desktop_version: runtime_env("AGENTDASH_RECOMMENDED_DESKTOP_VERSION")
            .unwrap_or_else(|| version.to_string()),
        relay_protocol_version: runtime_env("AGENTDASH_RELAY_PROTOCOL_VERSION")
            .unwrap_or_else(|| DEFAULT_RELAY_PROTOCOL_VERSION.to_string()),
    }
}

fn configured_public_origin() -> String {
    configured_public_origin_from_env()
        .unwrap_or_else(derived_local_origin)
        .trim_end_matches('/')
        .to_string()
}

fn configured_public_origin_from_env() -> Option<String> {
    configured_public_origin_from_value(runtime_env("AGENTDASH_PUBLIC_ORIGIN"))
}

fn configured_public_origin_from_value(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim_end_matches('/').to_string())
}

pub(crate) fn derive_relay_ws_url(server_origin: &str) -> String {
    if let Some(rest) = server_origin.strip_prefix("https://") {
        return format!("wss://{rest}/ws/backend");
    }
    if let Some(rest) = server_origin.strip_prefix("http://") {
        return format!("ws://{rest}/ws/backend");
    }
    format!("{server_origin}/ws/backend")
}

fn derived_local_origin() -> String {
    let host = runtime_env("AGENTDASH_BIND_HOST")
        .or_else(|| runtime_env("HOST"))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let origin_host = if host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        &host
    };
    let port = runtime_env("AGENTDASH_PORT")
        .or_else(|| runtime_env("PORT"))
        .unwrap_or_else(|| "3001".to_string());
    format!("http://{origin_host}:{port}")
}

fn runtime_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{configured_public_origin_from_value, derive_relay_ws_url};

    #[test]
    fn relay_ws_url_uses_ws_for_http_origin() {
        assert_eq!(
            derive_relay_ws_url("http://agentdash.example.internal"),
            "ws://agentdash.example.internal/ws/backend"
        );
    }

    #[test]
    fn relay_ws_url_uses_wss_for_https_origin() {
        assert_eq!(
            derive_relay_ws_url("https://agentdash.example.internal"),
            "wss://agentdash.example.internal/ws/backend"
        );
    }

    #[test]
    fn configured_public_origin_only_uses_public_origin_value() {
        assert_eq!(
            configured_public_origin_from_value(Some("http://127.0.0.1:3001/".to_string())),
            Some("http://127.0.0.1:3001".to_string())
        );
        assert_eq!(configured_public_origin_from_value(None), None);
    }
}
