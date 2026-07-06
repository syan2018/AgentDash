use std::{
    cmp::Ordering,
    collections::BTreeMap,
    future::Future,
    sync::OnceLock,
    time::{Duration, Instant},
};

use agentdash_contracts::desktop_release::{
    DesktopManualInstallerArtifact, DesktopUpdateArtifact, DesktopUpdateCheckQuery,
    DesktopUpdateCheckResponse, DesktopUpdateDiagnostics, DesktopUpdatePolicy,
    DesktopUpdateRecommendedVersionSource, DesktopUpdateRelease, DesktopUpdateStatus,
};
use axum::{Json, Router, extract::Query, http::StatusCode, response::IntoResponse, routing::get};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use url::Url;

use crate::app_state::AppState;

const PRODUCT_NAME: &str = "AgentDash";
const DEFAULT_RELAY_PROTOCOL_VERSION: &str = "1";
const DESKTOP_UPDATE_CHANNEL: &str = "stable";
const DEFAULT_DESKTOP_UPDATE_PLATFORM: &str = "windows";
const DEFAULT_DESKTOP_UPDATE_ARCH: &str = "x86_64";
const DEFAULT_DESKTOP_MANIFEST_CACHE_TTL_SECONDS: u64 = 60;
const DESKTOP_STABLE_MANIFEST_URL_ENV: &str = "AGENTDASH_DESKTOP_STABLE_MANIFEST_URL";
const DESKTOP_MANIFEST_CACHE_TTL_SECONDS_ENV: &str = "AGENTDASH_DESKTOP_MANIFEST_CACHE_TTL_SECONDS";
const MIN_DESKTOP_VERSION_ENV: &str = "AGENTDASH_MIN_DESKTOP_VERSION";
const RECOMMENDED_DESKTOP_VERSION_ENV: &str = "AGENTDASH_RECOMMENDED_DESKTOP_VERSION";

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
    pub min_desktop_version: Option<String>,
    pub recommended_desktop_version: Option<String>,
    pub relay_protocol_version: String,
}

#[derive(Debug, Clone)]
struct DesktopUpdateRuntimeConfig {
    manifest_url: Option<String>,
    min_desktop_version: Option<String>,
    recommended_desktop_version: Option<String>,
    cache_ttl: Duration,
}

#[derive(Debug, Clone)]
struct ManifestCacheEntry {
    manifest_url: String,
    fetched_at: Instant,
    fetched_at_wire: String,
    manifest: StableDesktopManifest,
}

#[derive(Debug, Clone)]
enum ManifestLoadResult {
    Loaded {
        manifest: StableDesktopManifest,
        fetched_at: String,
        cache_hit: bool,
    },
    FetchFailed(String),
    InvalidManifest(String),
}

#[derive(Debug, Clone, Deserialize)]
struct StableDesktopManifest {
    product: String,
    version: String,
    channel: String,
    published_at: String,
    release_notes: String,
    platforms: BTreeMap<String, StableDesktopPlatformManifest>,
}

#[derive(Debug, Clone, Deserialize)]
struct StableDesktopPlatformManifest {
    platform: String,
    arch: String,
    updater: StableDesktopUpdaterArtifact,
    installer: StableDesktopInstallerArtifact,
}

#[derive(Debug, Clone, Deserialize)]
struct StableDesktopUpdaterArtifact {
    public_url: String,
    sha256: String,
    signature: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StableDesktopInstallerArtifact {
    public_url: String,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct TauriDesktopUpdateRelease {
    version: String,
    notes: String,
    pub_date: String,
    platforms: BTreeMap<String, TauriDesktopUpdatePlatform>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct TauriDesktopUpdatePlatform {
    url: String,
    signature: String,
}

static DESKTOP_STABLE_MANIFEST_CACHE: OnceLock<Mutex<Option<ManifestCacheEntry>>> = OnceLock::new();

pub fn router() -> Router<std::sync::Arc<AppState>> {
    Router::new()
        .route("/desktop/update", get(desktop_update_check))
        .route("/desktop/update/tauri", get(tauri_desktop_update_check))
}

pub async fn version_info() -> Json<VersionInfoResponse> {
    Json(build_version_info())
}

pub async fn agentdash_discovery() -> Json<AgentDashDiscoveryResponse> {
    Json(build_agentdash_discovery())
}

async fn desktop_update_check(
    Query(query): Query<DesktopUpdateCheckQuery>,
) -> Json<DesktopUpdateCheckResponse> {
    Json(build_desktop_update_response(query, DesktopUpdateRuntimeConfig::from_env()).await)
}

async fn tauri_desktop_update_check(
    Query(query): Query<DesktopUpdateCheckQuery>,
) -> impl IntoResponse {
    match build_tauri_desktop_update_release(query, DesktopUpdateRuntimeConfig::from_env()).await {
        Some(release) => Json(release).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
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
        min_desktop_version: runtime_env(MIN_DESKTOP_VERSION_ENV),
        recommended_desktop_version: runtime_env(RECOMMENDED_DESKTOP_VERSION_ENV),
        relay_protocol_version: runtime_env("AGENTDASH_RELAY_PROTOCOL_VERSION")
            .unwrap_or_else(|| DEFAULT_RELAY_PROTOCOL_VERSION.to_string()),
    }
}

async fn build_desktop_update_response(
    query: DesktopUpdateCheckQuery,
    config: DesktopUpdateRuntimeConfig,
) -> DesktopUpdateCheckResponse {
    let target = DesktopUpdateTarget::from_query(query);
    let Some(_) = config.manifest_url.as_ref() else {
        return build_desktop_update_response_from_load(target, &config, None);
    };
    let load = load_stable_manifest(&config).await;
    build_desktop_update_response_from_load(target, &config, Some(load))
}

async fn build_tauri_desktop_update_release(
    query: DesktopUpdateCheckQuery,
    config: DesktopUpdateRuntimeConfig,
) -> Option<TauriDesktopUpdateRelease> {
    let response = build_desktop_update_response(query, config).await;
    tauri_release_from_desktop_update_response(response)
}

fn tauri_release_from_desktop_update_response(
    response: DesktopUpdateCheckResponse,
) -> Option<TauriDesktopUpdateRelease> {
    if response.status != DesktopUpdateStatus::UpdateAvailable {
        return None;
    }

    let latest = response.latest?;
    let platform_key = format!("{}-{}", latest.platform, latest.arch);
    let platform = TauriDesktopUpdatePlatform {
        url: latest.updater.url,
        signature: latest.updater.signature,
    };
    let mut platforms = BTreeMap::new();
    platforms.insert(platform_key.clone(), platform.clone());
    if latest.platform == "windows" {
        platforms.insert(format!("{platform_key}-nsis"), platform);
    }

    Some(TauriDesktopUpdateRelease {
        version: latest.version,
        notes: latest.release_notes,
        pub_date: latest.published_at,
        platforms,
    })
}

fn build_desktop_update_response_from_load(
    target: DesktopUpdateTarget,
    config: &DesktopUpdateRuntimeConfig,
    load: Option<ManifestLoadResult>,
) -> DesktopUpdateCheckResponse {
    let base_policy = DesktopUpdatePolicy {
        min_desktop_version: config.min_desktop_version.clone(),
        recommended_desktop_version: config.recommended_desktop_version.clone(),
        min_desktop_version_configured: config.min_desktop_version.is_some(),
        recommended_desktop_version_source: if config.recommended_desktop_version.is_some() {
            DesktopUpdateRecommendedVersionSource::Environment
        } else {
            DesktopUpdateRecommendedVersionSource::None
        },
    };

    let base_diagnostics =
        |code: Option<&str>, message: Option<String>, cache_hit: bool| DesktopUpdateDiagnostics {
            manifest_url_configured: config.manifest_url.is_some(),
            code: code.map(str::to_string),
            message,
            fetched_at: None,
            cache_hit,
            cache_ttl_seconds: config.cache_ttl.as_secs(),
        };

    match load {
        None => DesktopUpdateCheckResponse {
            status: DesktopUpdateStatus::Unconfigured,
            channel: DESKTOP_UPDATE_CHANNEL.to_string(),
            platform: target.platform,
            arch: target.arch,
            current_version: target.current_version,
            latest: None,
            update_available: None,
            policy: base_policy,
            diagnostics: base_diagnostics(
                Some("desktop_manifest_url_unconfigured"),
                Some(format!(
                    "{DESKTOP_STABLE_MANIFEST_URL_ENV} is not configured"
                )),
                false,
            ),
        },
        Some(ManifestLoadResult::FetchFailed(message)) => DesktopUpdateCheckResponse {
            status: DesktopUpdateStatus::FetchFailed,
            channel: DESKTOP_UPDATE_CHANNEL.to_string(),
            platform: target.platform,
            arch: target.arch,
            current_version: target.current_version,
            latest: None,
            update_available: None,
            policy: base_policy,
            diagnostics: base_diagnostics(
                Some("desktop_manifest_fetch_failed"),
                Some(message),
                false,
            ),
        },
        Some(ManifestLoadResult::InvalidManifest(message)) => DesktopUpdateCheckResponse {
            status: DesktopUpdateStatus::InvalidManifest,
            channel: DESKTOP_UPDATE_CHANNEL.to_string(),
            platform: target.platform,
            arch: target.arch,
            current_version: target.current_version,
            latest: None,
            update_available: None,
            policy: base_policy,
            diagnostics: base_diagnostics(Some("desktop_manifest_invalid"), Some(message), false),
        },
        Some(ManifestLoadResult::Loaded {
            manifest,
            fetched_at,
            cache_hit,
        }) => build_loaded_manifest_response(target, config, manifest, fetched_at, cache_hit),
    }
}

fn build_loaded_manifest_response(
    target: DesktopUpdateTarget,
    config: &DesktopUpdateRuntimeConfig,
    manifest: StableDesktopManifest,
    fetched_at: String,
    cache_hit: bool,
) -> DesktopUpdateCheckResponse {
    let recommended_desktop_version_source = if config.recommended_desktop_version.is_some() {
        DesktopUpdateRecommendedVersionSource::Environment
    } else {
        DesktopUpdateRecommendedVersionSource::Manifest
    };
    let policy = DesktopUpdatePolicy {
        min_desktop_version: config.min_desktop_version.clone(),
        recommended_desktop_version: config
            .recommended_desktop_version
            .clone()
            .or_else(|| Some(manifest.version.clone())),
        min_desktop_version_configured: config.min_desktop_version.is_some(),
        recommended_desktop_version_source,
    };

    let platform_key = target.platform_key();
    let Some(platform_manifest) = manifest.platforms.get(&platform_key) else {
        return DesktopUpdateCheckResponse {
            status: DesktopUpdateStatus::UnsupportedTarget,
            channel: manifest.channel,
            platform: target.platform,
            arch: target.arch,
            current_version: target.current_version,
            latest: None,
            update_available: None,
            policy,
            diagnostics: DesktopUpdateDiagnostics {
                manifest_url_configured: true,
                code: Some("desktop_manifest_target_unsupported".to_string()),
                message: Some(format!(
                    "manifest does not contain platform target {platform_key}"
                )),
                fetched_at: Some(fetched_at),
                cache_hit,
                cache_ttl_seconds: config.cache_ttl.as_secs(),
            },
        };
    };

    let update_available = target
        .current_version
        .as_deref()
        .map(|current| version_cmp(current, &manifest.version) == Ordering::Less);
    let status = match update_available {
        Some(true) => DesktopUpdateStatus::UpdateAvailable,
        Some(false) => DesktopUpdateStatus::UpToDate,
        None => DesktopUpdateStatus::LatestAvailable,
    };
    let latest = DesktopUpdateRelease {
        product: manifest.product,
        channel: manifest.channel.clone(),
        version: manifest.version,
        platform: platform_manifest.platform.clone(),
        arch: platform_manifest.arch.clone(),
        published_at: manifest.published_at,
        release_notes: manifest.release_notes,
        updater: DesktopUpdateArtifact {
            url: platform_manifest.updater.public_url.clone(),
            sha256: platform_manifest.updater.sha256.clone(),
            signature: platform_manifest.updater.signature.clone(),
        },
        manual_installer: DesktopManualInstallerArtifact {
            url: platform_manifest.installer.public_url.clone(),
            sha256: platform_manifest.installer.sha256.clone(),
        },
    };

    DesktopUpdateCheckResponse {
        status,
        channel: manifest.channel,
        platform: target.platform,
        arch: target.arch,
        current_version: target.current_version,
        latest: Some(latest),
        update_available,
        policy,
        diagnostics: DesktopUpdateDiagnostics {
            manifest_url_configured: true,
            code: None,
            message: None,
            fetched_at: Some(fetched_at),
            cache_hit,
            cache_ttl_seconds: config.cache_ttl.as_secs(),
        },
    }
}

async fn load_stable_manifest(config: &DesktopUpdateRuntimeConfig) -> ManifestLoadResult {
    load_stable_manifest_with_fetcher(config, fetch_manifest_http).await
}

async fn load_stable_manifest_with_fetcher<F, Fut>(
    config: &DesktopUpdateRuntimeConfig,
    fetcher: F,
) -> ManifestLoadResult
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let Some(manifest_url) = config.manifest_url.clone() else {
        return ManifestLoadResult::FetchFailed(format!(
            "{DESKTOP_STABLE_MANIFEST_URL_ENV} is not configured"
        ));
    };

    if let Some(cached) = cached_manifest(&manifest_url, config.cache_ttl).await {
        return ManifestLoadResult::Loaded {
            manifest: cached.manifest,
            fetched_at: cached.fetched_at_wire,
            cache_hit: true,
        };
    }

    let raw = match fetcher(manifest_url.clone()).await {
        Ok(raw) => raw,
        Err(message) => return ManifestLoadResult::FetchFailed(message),
    };
    let manifest = match parse_stable_manifest(&raw) {
        Ok(manifest) => manifest,
        Err(message) => return ManifestLoadResult::InvalidManifest(message),
    };
    let fetched_at_wire = Utc::now().to_rfc3339();
    store_cached_manifest(ManifestCacheEntry {
        manifest_url,
        fetched_at: Instant::now(),
        fetched_at_wire: fetched_at_wire.clone(),
        manifest: manifest.clone(),
    })
    .await;

    ManifestLoadResult::Loaded {
        manifest,
        fetched_at: fetched_at_wire,
        cache_hit: false,
    }
}

async fn fetch_manifest_http(manifest_url: String) -> Result<String, String> {
    let url = Url::parse(&manifest_url)
        .map_err(|error| format!("desktop stable manifest URL is invalid: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("desktop stable manifest URL must use http or https".to_string());
    }

    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| format!("desktop stable manifest fetch failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "desktop stable manifest fetch returned HTTP {}",
            status.as_u16()
        ));
    }
    response
        .text()
        .await
        .map_err(|error| format!("desktop stable manifest body read failed: {error}"))
}

async fn cached_manifest(manifest_url: &str, cache_ttl: Duration) -> Option<ManifestCacheEntry> {
    let guard = desktop_manifest_cache().lock().await;
    let cached = guard.as_ref()?;
    if cached.manifest_url == manifest_url && cached.fetched_at.elapsed() <= cache_ttl {
        return Some(cached.clone());
    }
    None
}

async fn store_cached_manifest(entry: ManifestCacheEntry) {
    let mut guard = desktop_manifest_cache().lock().await;
    *guard = Some(entry);
}

fn desktop_manifest_cache() -> &'static Mutex<Option<ManifestCacheEntry>> {
    DESKTOP_STABLE_MANIFEST_CACHE.get_or_init(|| Mutex::new(None))
}

fn parse_stable_manifest(raw: &str) -> Result<StableDesktopManifest, String> {
    let manifest: StableDesktopManifest = serde_json::from_str(raw)
        .map_err(|error| format!("desktop stable manifest JSON is invalid: {error}"))?;
    validate_stable_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_stable_manifest(manifest: &StableDesktopManifest) -> Result<(), String> {
    validate_non_empty("product", &manifest.product)?;
    validate_non_empty("version", &manifest.version)?;
    validate_non_empty("channel", &manifest.channel)?;
    validate_non_empty("published_at", &manifest.published_at)?;
    if manifest.channel != DESKTOP_UPDATE_CHANNEL {
        return Err(format!(
            "manifest channel must be {DESKTOP_UPDATE_CHANNEL}, got {}",
            manifest.channel
        ));
    }
    if manifest.platforms.is_empty() {
        return Err("manifest platforms must not be empty".to_string());
    }

    for (target, platform) in &manifest.platforms {
        validate_non_empty(&format!("platforms.{target}.platform"), &platform.platform)?;
        validate_non_empty(&format!("platforms.{target}.arch"), &platform.arch)?;
        if target != &format!("{}-{}", platform.platform, platform.arch) {
            return Err(format!(
                "platforms.{target} must match platform and arch fields"
            ));
        }
        validate_http_url(
            &format!("platforms.{target}.updater.public_url"),
            &platform.updater.public_url,
        )?;
        validate_sha256(
            &format!("platforms.{target}.updater.sha256"),
            &platform.updater.sha256,
        )?;
        validate_non_empty(
            &format!("platforms.{target}.updater.signature"),
            &platform.updater.signature,
        )?;
        validate_http_url(
            &format!("platforms.{target}.installer.public_url"),
            &platform.installer.public_url,
        )?;
        validate_sha256(
            &format!("platforms.{target}.installer.sha256"),
            &platform.installer.sha256,
        )?;
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    Ok(())
}

fn validate_http_url(field: &str, value: &str) -> Result<(), String> {
    validate_non_empty(field, value)?;
    let url = Url::parse(value).map_err(|error| format!("{field} is not a valid URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("{field} must use http or https"));
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> Result<(), String> {
    validate_non_empty(field, value)?;
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{field} must be a sha256 digest"));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct DesktopUpdateTarget {
    platform: String,
    arch: String,
    current_version: Option<String>,
}

impl DesktopUpdateTarget {
    fn from_query(query: DesktopUpdateCheckQuery) -> Self {
        Self {
            platform: normalize_query_value(query.platform)
                .unwrap_or_else(|| DEFAULT_DESKTOP_UPDATE_PLATFORM.to_string()),
            arch: normalize_query_value(query.arch)
                .unwrap_or_else(|| DEFAULT_DESKTOP_UPDATE_ARCH.to_string()),
            current_version: normalize_query_value(query.current_version),
        }
    }

    fn platform_key(&self) -> String {
        format!("{}-{}", self.platform, self.arch)
    }
}

impl DesktopUpdateRuntimeConfig {
    fn from_env() -> Self {
        Self {
            manifest_url: runtime_env(DESKTOP_STABLE_MANIFEST_URL_ENV),
            min_desktop_version: runtime_env(MIN_DESKTOP_VERSION_ENV),
            recommended_desktop_version: runtime_env(RECOMMENDED_DESKTOP_VERSION_ENV),
            cache_ttl: desktop_manifest_cache_ttl_from_env(),
        }
    }

    #[cfg(test)]
    fn from_values(
        manifest_url: Option<&str>,
        min_desktop_version: Option<&str>,
        recommended_desktop_version: Option<&str>,
    ) -> Self {
        Self {
            manifest_url: manifest_url.map(str::to_string),
            min_desktop_version: min_desktop_version.map(str::to_string),
            recommended_desktop_version: recommended_desktop_version.map(str::to_string),
            cache_ttl: Duration::from_secs(DEFAULT_DESKTOP_MANIFEST_CACHE_TTL_SECONDS),
        }
    }

    #[cfg(test)]
    fn with_cache_ttl(mut self, cache_ttl: Duration) -> Self {
        self.cache_ttl = cache_ttl;
        self
    }
}

fn desktop_manifest_cache_ttl_from_env() -> Duration {
    let seconds = runtime_env(DESKTOP_MANIFEST_CACHE_TTL_SECONDS_ENV)
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_DESKTOP_MANIFEST_CACHE_TTL_SECONDS);
    Duration::from_secs(seconds)
}

fn normalize_query_value(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn version_cmp(left: &str, right: &str) -> Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    let max_len = left_parts.len().max(right_parts.len());
    for index in 0..max_len {
        let left = left_parts.get(index).map(String::as_str).unwrap_or("0");
        let right = right_parts.get(index).map(String::as_str).unwrap_or("0");
        let ordering = match (left.parse::<u64>(), right.parse::<u64>()) {
            (Ok(left), Ok(right)) => left.cmp(&right),
            _ => left.cmp(right),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

fn version_parts(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('v')
        .split(['.', '-', '+', '_'])
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
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
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering as AtomicOrdering},
        },
        time::Duration,
    };

    use super::{
        DesktopUpdateRuntimeConfig, ManifestLoadResult, build_desktop_update_response,
        build_desktop_update_response_from_load, cached_manifest,
        configured_public_origin_from_value, derive_relay_ws_url,
        load_stable_manifest_with_fetcher, parse_stable_manifest, store_cached_manifest,
        version_cmp,
    };
    use agentdash_contracts::desktop_release::{
        DesktopUpdateCheckQuery, DesktopUpdateRecommendedVersionSource, DesktopUpdateStatus,
    };

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

    #[tokio::test]
    async fn desktop_update_unconfigured_returns_diagnostic_without_latest_release() {
        let response = build_desktop_update_response(
            DesktopUpdateCheckQuery {
                platform: None,
                arch: None,
                current_version: Some("0.1.0".to_string()),
            },
            DesktopUpdateRuntimeConfig::from_values(None, None, None),
        )
        .await;

        assert_eq!(response.status, DesktopUpdateStatus::Unconfigured);
        assert_eq!(response.platform, "windows");
        assert_eq!(response.arch, "x86_64");
        assert!(response.latest.is_none());
        assert_eq!(response.policy.min_desktop_version, None);
        assert!(!response.policy.min_desktop_version_configured);
        assert_eq!(
            response.diagnostics.code.as_deref(),
            Some("desktop_manifest_url_unconfigured")
        );
    }

    #[tokio::test]
    async fn desktop_update_fetch_failure_returns_diagnostic_response() {
        let config = DesktopUpdateRuntimeConfig::from_values(
            Some("https://updates.example/latest.json"),
            None,
            None,
        );
        let response = build_desktop_update_response_from_load(
            query_target("0.1.0"),
            &config,
            Some(ManifestLoadResult::FetchFailed(
                "network unavailable".to_string(),
            )),
        );

        assert_eq!(response.status, DesktopUpdateStatus::FetchFailed);
        assert!(response.latest.is_none());
        assert_eq!(
            response.diagnostics.code.as_deref(),
            Some("desktop_manifest_fetch_failed")
        );
        assert_eq!(
            response.diagnostics.message.as_deref(),
            Some("network unavailable")
        );
    }

    #[tokio::test]
    async fn desktop_update_invalid_manifest_returns_diagnostic_response() {
        let config = DesktopUpdateRuntimeConfig::from_values(
            Some("https://updates.example/latest.json"),
            None,
            None,
        );
        let response = build_desktop_update_response_from_load(
            query_target("0.1.0"),
            &config,
            Some(ManifestLoadResult::InvalidManifest(
                "bad schema".to_string(),
            )),
        );

        assert_eq!(response.status, DesktopUpdateStatus::InvalidManifest);
        assert!(response.latest.is_none());
        assert_eq!(
            response.diagnostics.code.as_deref(),
            Some("desktop_manifest_invalid")
        );
    }

    #[tokio::test]
    async fn desktop_update_success_maps_manifest_and_env_policy() {
        let manifest = parse_stable_manifest(&valid_manifest("0.2.0")).expect("manifest");
        let config = DesktopUpdateRuntimeConfig::from_values(
            Some("https://updates.example/latest.json"),
            Some("0.1.5"),
            Some("0.1.9"),
        );
        let response = build_desktop_update_response_from_load(
            query_target("0.1.0"),
            &config,
            Some(ManifestLoadResult::Loaded {
                manifest,
                fetched_at: "2026-07-06T00:00:00Z".to_string(),
                cache_hit: false,
            }),
        );

        assert_eq!(response.status, DesktopUpdateStatus::UpdateAvailable);
        assert_eq!(response.update_available, Some(true));
        assert_eq!(
            response
                .latest
                .as_ref()
                .map(|latest| latest.version.as_str()),
            Some("0.2.0")
        );
        assert_eq!(
            response
                .latest
                .as_ref()
                .map(|latest| latest.updater.url.as_str()),
            Some("https://updates.example/releases/0.2.0/AgentDash_0.2.0_x64.nsis.zip")
        );
        assert_eq!(
            response.policy.min_desktop_version.as_deref(),
            Some("0.1.5")
        );
        assert!(response.policy.min_desktop_version_configured);
        assert_eq!(
            response.policy.recommended_desktop_version.as_deref(),
            Some("0.1.9")
        );
        assert_eq!(
            response.policy.recommended_desktop_version_source,
            DesktopUpdateRecommendedVersionSource::Environment
        );
    }

    #[tokio::test]
    async fn desktop_update_uses_manifest_recommended_version_when_env_is_absent() {
        let manifest = parse_stable_manifest(&valid_manifest("0.2.0")).expect("manifest");
        let config = DesktopUpdateRuntimeConfig::from_values(
            Some("https://updates.example/latest.json"),
            None,
            None,
        );
        let response = build_desktop_update_response_from_load(
            query_target("0.2.0"),
            &config,
            Some(ManifestLoadResult::Loaded {
                manifest,
                fetched_at: "2026-07-06T00:00:00Z".to_string(),
                cache_hit: true,
            }),
        );

        assert_eq!(response.status, DesktopUpdateStatus::UpToDate);
        assert_eq!(response.update_available, Some(false));
        assert_eq!(
            response.policy.recommended_desktop_version.as_deref(),
            Some("0.2.0")
        );
        assert_eq!(
            response.policy.recommended_desktop_version_source,
            DesktopUpdateRecommendedVersionSource::Manifest
        );
        assert!(response.diagnostics.cache_hit);
    }

    #[tokio::test]
    async fn tauri_update_release_maps_available_update_to_native_schema() {
        let manifest = parse_stable_manifest(&valid_manifest("0.2.0")).expect("manifest");
        let config = DesktopUpdateRuntimeConfig::from_values(
            Some("https://updates.example/latest.json"),
            None,
            None,
        );
        let response = build_desktop_update_response_from_load(
            query_target("0.1.0"),
            &config,
            Some(ManifestLoadResult::Loaded {
                manifest,
                fetched_at: "2026-07-06T00:00:00Z".to_string(),
                cache_hit: false,
            }),
        );
        let release = super::tauri_release_from_desktop_update_response(response)
            .expect("tauri release should be available");

        assert_eq!(release.version, "0.2.0");
        assert_eq!(release.notes, "Desktop updater test release");
        assert_eq!(release.pub_date, "2026-07-06T00:00:00Z");
        assert_eq!(
            release
                .platforms
                .get("windows-x86_64-nsis")
                .map(|platform| platform.url.as_str()),
            Some("https://updates.example/releases/0.2.0/AgentDash_0.2.0_x64.nsis.zip")
        );
        assert_eq!(
            release
                .platforms
                .get("windows-x86_64")
                .map(|platform| platform.signature.as_str()),
            Some("test-signature")
        );
    }

    #[tokio::test]
    async fn tauri_update_release_is_empty_when_product_endpoint_has_no_update() {
        let manifest = parse_stable_manifest(&valid_manifest("0.2.0")).expect("manifest");
        let config = DesktopUpdateRuntimeConfig::from_values(
            Some("https://updates.example/latest.json"),
            None,
            None,
        );
        let response = build_desktop_update_response_from_load(
            query_target("0.2.0"),
            &config,
            Some(ManifestLoadResult::Loaded {
                manifest,
                fetched_at: "2026-07-06T00:00:00Z".to_string(),
                cache_hit: false,
            }),
        );

        assert!(super::tauri_release_from_desktop_update_response(response).is_none());
    }

    #[test]
    fn manifest_validation_rejects_missing_target_artifact_signature() {
        let raw = valid_manifest("0.2.0").replace(r#""signature":"#, r#""signature_missing":"#);
        let error = parse_stable_manifest(&raw).expect_err("invalid manifest");
        assert!(error.contains("missing field `signature`"));
    }

    #[tokio::test]
    async fn manifest_loader_uses_short_cache_for_same_url() {
        let url = "https://updates.example/cache-test/latest.json";
        let config = DesktopUpdateRuntimeConfig::from_values(Some(url), None, None)
            .with_cache_ttl(Duration::from_secs(60));
        let calls = Arc::new(AtomicUsize::new(0));

        let first_calls = calls.clone();
        let first = load_stable_manifest_with_fetcher(&config, move |_| {
            let first_calls = first_calls.clone();
            async move {
                first_calls.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(valid_manifest("0.3.0"))
            }
        })
        .await;
        assert!(matches!(
            first,
            ManifestLoadResult::Loaded {
                cache_hit: false,
                ..
            }
        ));

        let second_calls = calls.clone();
        let second = load_stable_manifest_with_fetcher(&config, move |_| {
            let second_calls = second_calls.clone();
            async move {
                second_calls.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(valid_manifest("0.4.0"))
            }
        })
        .await;

        assert!(matches!(
            second,
            ManifestLoadResult::Loaded {
                cache_hit: true,
                ..
            }
        ));
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
    }

    #[tokio::test]
    async fn manifest_cache_expires_after_ttl() {
        let url = "https://updates.example/expired-cache-test/latest.json";
        let manifest = parse_stable_manifest(&valid_manifest("0.2.0")).expect("manifest");
        store_cached_manifest(super::ManifestCacheEntry {
            manifest_url: url.to_string(),
            fetched_at: std::time::Instant::now() - Duration::from_secs(120),
            fetched_at_wire: "2026-07-06T00:00:00Z".to_string(),
            manifest,
        })
        .await;

        let cached = cached_manifest(url, Duration::from_secs(60)).await;
        assert!(cached.is_none());
    }

    #[test]
    fn version_comparison_handles_numeric_components() {
        assert_eq!(version_cmp("0.10.0", "0.2.0"), std::cmp::Ordering::Greater);
        assert_eq!(version_cmp("v0.1.0", "0.1.0"), std::cmp::Ordering::Equal);
        assert_eq!(version_cmp("0.1.0", "0.1.1"), std::cmp::Ordering::Less);
    }

    fn query_target(current_version: &str) -> super::DesktopUpdateTarget {
        super::DesktopUpdateTarget::from_query(DesktopUpdateCheckQuery {
            platform: Some("windows".to_string()),
            arch: Some("x86_64".to_string()),
            current_version: Some(current_version.to_string()),
        })
    }

    fn valid_manifest(version: &str) -> String {
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        format!(
            r#"{{
  "product": "AgentDash",
  "version": "{version}",
  "channel": "stable",
  "published_at": "2026-07-06T00:00:00Z",
  "release_notes": "Desktop updater test release",
  "platforms": {{
    "windows-x86_64": {{
      "platform": "windows",
      "arch": "x86_64",
      "updater": {{
        "public_url": "https://updates.example/releases/{version}/AgentDash_{version}_x64.nsis.zip",
        "sha256": "sha256:{hex}",
        "signature": "test-signature"
      }},
      "installer": {{
        "public_url": "https://updates.example/releases/{version}/AgentDash_{version}_x64-setup.exe",
        "sha256": "sha256:{hex}"
      }}
    }}
  }}
}}"#
        )
    }
}
