use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value, json};

use super::LocalExtensionHostError;
use super::host_api::{DEFAULT_HOST_API_TIMEOUT_MS, optional_string, optional_u64, require_string};
use super::permission_guard::require_declared_permission;
use super::process::ActiveExtension;

pub(super) async fn resolve_http_fetch(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    let url = require_string(params, "url")?;
    let parsed = reqwest::Url::parse(&url)
        .map_err(|error| LocalExtensionHostError::Host(format!("http.fetch URL 非法: {error}")))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(LocalExtensionHostError::Host(format!(
                "http.fetch 不支持 URL scheme: {scheme}"
            )));
        }
    }
    let host = parsed.host_str().unwrap_or_default().to_string();
    let permissions = vec!["http.fetch".to_string(), format!("http.fetch:{host}")];
    require_declared_permission(active, params, &permissions)?;

    let options = params.get("options").unwrap_or(&Value::Null);
    let method = optional_string(options, "method").unwrap_or_else(|| "GET".to_string());
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|error| {
        LocalExtensionHostError::Host(format!("http.fetch method 非法: {error}"))
    })?;
    let timeout_ms = optional_u64(options, "timeout_ms").unwrap_or(DEFAULT_HOST_API_TIMEOUT_MS);
    let mut request = reqwest::Client::new().request(method, parsed);
    if let Some(headers) = options.get("headers").and_then(Value::as_object) {
        request = request.headers(parse_headers(headers)?);
    }
    if let Some(body) = options.get("body") {
        request = if let Some(text) = body.as_str() {
            request.body(text.to_string())
        } else {
            request.body(body.to_string())
        };
    }
    let response =
        tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), request.send())
            .await
            .map_err(|_| LocalExtensionHostError::Host("http.fetch timeout".to_string()))?
            .map_err(|error| LocalExtensionHostError::Host(format!("http.fetch 失败: {error}")))?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(key, value)| {
            (
                key.as_str().to_string(),
                Value::String(value.to_str().unwrap_or_default().to_string()),
            )
        })
        .collect::<Map<String, Value>>();
    let body = response.text().await.map_err(|error| {
        LocalExtensionHostError::Host(format!("http.fetch 读取响应失败: {error}"))
    })?;
    Ok(json!({
        "status": status,
        "headers": headers,
        "body": body,
    }))
}

fn parse_headers(headers: &Map<String, Value>) -> Result<HeaderMap, LocalExtensionHostError> {
    let mut map = HeaderMap::new();
    for (key, value) in headers {
        let Some(value) = value.as_str() else {
            return Err(LocalExtensionHostError::Host(format!(
                "http.fetch header `{key}` 必须是字符串"
            )));
        };
        let name = HeaderName::from_bytes(key.as_bytes()).map_err(|error| {
            LocalExtensionHostError::Host(format!("http.fetch header name 非法: {error}"))
        })?;
        let value = HeaderValue::from_str(value).map_err(|error| {
            LocalExtensionHostError::Host(format!("http.fetch header value 非法: {error}"))
        })?;
        map.insert(name, value);
    }
    Ok(map)
}
