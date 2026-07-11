use std::future::Future;
use std::pin::Pin;

use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use agentdash_agent::bridge::{BridgeError, ProviderErrorClassification, StreamChunk};

/// 各 bridge `stream_complete` 共用的流脚手架：建立 64 容量 channel、`tokio::spawn`
/// 运行 `run`，并在其返回 `Err` 时把错误作为 `StreamChunk::Error` 转发给消费方。
pub(super) fn spawn_bridge_stream<F, Fut>(
    run: F,
) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>
where
    F: FnOnce(Sender<StreamChunk>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), BridgeError>> + Send,
{
    let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

    tokio::spawn(async move {
        if let Err(error) = run(tx.clone()).await {
            let _ = tx.send(StreamChunk::Error(error)).await;
        }
    });

    Box::pin(ReceiverStream::new(rx))
}

/// 校验 HTTP 响应状态：非 2xx 时读出 body 并组装统一的 `{api_label} 返回 {status}: {body}`
/// 错误。`api_label` 保留各 bridge 既有前缀（如 `"API"`）。
pub(super) async fn check_http_response(
    response: reqwest::Response,
    api_label: &str,
) -> Result<reqwest::Response, BridgeError> {
    check_http_response_with_body_message(response, api_label, |body| body.to_string()).await
}

pub(super) async fn check_http_response_with_body_message(
    response: reqwest::Response,
    api_label: &str,
    body_message: impl FnOnce(&str) -> String,
) -> Result<reqwest::Response, BridgeError> {
    if !response.status().is_success() {
        let status = response.status();
        let headers = response.headers().clone();
        let body_text = response.text().await.unwrap_or_default();
        let display_body = body_message(&body_text);
        return Err(provider_http_error(
            api_label,
            status,
            &headers,
            &body_text,
            display_body,
        ));
    }
    Ok(response)
}

pub(super) fn provider_transport_error(context: &str, error: reqwest::Error) -> BridgeError {
    let provider_code = if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connection_error"
    } else if error.is_body() {
        "stream_transport_error"
    } else {
        "transport_error"
    };
    BridgeError::provider(
        format!("{context}: {error}"),
        ProviderErrorClassification::retryable().with_provider_code(provider_code),
    )
}

pub(super) fn provider_stream_read_error(context: &str, error: reqwest::Error) -> BridgeError {
    let provider_code = if error.is_timeout() {
        "stream_timeout"
    } else {
        "stream_disconnected"
    };
    BridgeError::provider(
        format!("{context}: {error}"),
        ProviderErrorClassification::retryable().with_provider_code(provider_code),
    )
}

pub(super) fn provider_event_error(
    message: impl Into<String>,
    raw_provider_body: Option<&str>,
) -> BridgeError {
    let message = message.into();
    let classification = classify_provider_event_failure(&message, raw_provider_body);
    BridgeError::provider(message, classification)
}

pub(super) fn provider_fatal_error(message: impl Into<String>, code: &'static str) -> BridgeError {
    BridgeError::provider(
        message,
        ProviderErrorClassification::fatal().with_provider_code(code),
    )
}

fn provider_http_error(
    api_label: &str,
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    raw_body: &str,
    display_body: String,
) -> BridgeError {
    let classification = classify_http_provider_failure(status, headers, raw_body);
    BridgeError::provider(
        format!("{api_label} 返回 {status}: {display_body}"),
        classification,
    )
}

pub(super) fn classify_http_provider_failure(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    body: &str,
) -> ProviderErrorClassification {
    let status_u16 = status.as_u16();
    let provider_code =
        provider_code_from_body(body).unwrap_or_else(|| provider_code_for_http_status(status));
    let body_lower = body.to_ascii_lowercase();

    let mut classification = if is_fatal_provider_code(&provider_code, &body_lower)
        || matches!(status_u16, 400 | 401 | 403 | 404 | 422)
    {
        ProviderErrorClassification::fatal()
    } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
    {
        ProviderErrorClassification::retryable()
    } else {
        ProviderErrorClassification::fatal()
    }
    .with_http_status(status_u16)
    .with_provider_code(provider_code);

    if let Some(retry_after_ms) = retry_after_ms_from_headers(headers) {
        classification = classification.with_retry_after_ms(retry_after_ms);
    }

    classification
}

fn classify_provider_event_failure(
    message: &str,
    raw_provider_body: Option<&str>,
) -> ProviderErrorClassification {
    let body = raw_provider_body.unwrap_or(message);
    let provider_code = provider_code_from_body(body).unwrap_or_else(|| {
        if looks_retryable_text(message) || looks_retryable_text(body) {
            "transient_provider_error".to_string()
        } else {
            "provider_error".to_string()
        }
    });
    let body_lower = body.to_ascii_lowercase();
    let message_lower = message.to_ascii_lowercase();

    if is_fatal_provider_code(&provider_code, &body_lower) {
        ProviderErrorClassification::fatal().with_provider_code(provider_code)
    } else if looks_retryable_text(&message_lower) || looks_retryable_text(&body_lower) {
        ProviderErrorClassification::retryable().with_provider_code(provider_code)
    } else {
        ProviderErrorClassification::fatal().with_provider_code(provider_code)
    }
}

fn retry_after_ms_from_headers(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    header_value(headers, reqwest::header::RETRY_AFTER.as_str())
        .and_then(parse_retry_after_ms)
        .or_else(|| header_value(headers, "x-ratelimit-reset-after").and_then(parse_seconds_ms))
        .or_else(|| header_value(headers, "x-ratelimit-reset").and_then(parse_unix_reset_ms))
}

fn header_value<'a>(headers: &'a reqwest::header::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn parse_retry_after_ms(value: &str) -> Option<u64> {
    parse_seconds_ms(value).or_else(|| {
        chrono::DateTime::parse_from_rfc2822(value)
            .ok()
            .and_then(|date| {
                let millis = date
                    .with_timezone(&chrono::Utc)
                    .signed_duration_since(chrono::Utc::now())
                    .num_milliseconds();
                u64::try_from(millis.max(0)).ok()
            })
    })
}

fn parse_seconds_ms(value: &str) -> Option<u64> {
    let seconds = value.trim().parse::<f64>().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some((seconds * 1000.0).round() as u64)
}

fn parse_unix_reset_ms(value: &str) -> Option<u64> {
    let raw = value.trim().parse::<i64>().ok()?;
    let reset_ms = if raw > 10_000_000_000 {
        raw
    } else {
        raw * 1000
    };
    u64::try_from((reset_ms - chrono::Utc::now().timestamp_millis()).max(0)).ok()
}

fn provider_code_for_http_status(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        400 => "invalid_request",
        401 | 403 => "auth_error",
        408 => "timeout",
        429 => "rate_limited",
        500..=599 => "provider_5xx",
        value => return format!("http_{value}"),
    }
    .to_string()
}

fn provider_code_from_body(body: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    let candidates = [
        pointer_string(&parsed, "/error/code"),
        pointer_string(&parsed, "/error/type"),
        pointer_string(&parsed, "/error/status"),
        pointer_string(&parsed, "/code"),
        pointer_string(&parsed, "/type"),
        pointer_string(&parsed, "/status"),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty())
}

fn pointer_string(value: &serde_json::Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn is_fatal_provider_code(code: &str, body_lower: &str) -> bool {
    let code = code.to_ascii_lowercase();
    let fatal_needles = [
        "auth",
        "invalid_api_key",
        "invalid_request",
        "context_length",
        "context_window",
        "usage_limit",
        "insufficient_quota",
        "quota",
        "billing",
        "permission",
        "forbidden",
    ];
    fatal_needles
        .iter()
        .any(|needle| code.contains(needle) || body_lower.contains(needle))
}

fn looks_retryable_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let retryable_needles = [
        "429",
        "500",
        "502",
        "503",
        "504",
        "rate limit",
        "rate_limit",
        "ratelimit",
        "overloaded",
        "service unavailable",
        "temporarily unavailable",
        "timeout",
        "timed out",
        "connection reset",
        "connection refused",
        "connection closed",
        "connection aborted",
        "stream disconnected",
        "stream ended",
        "server error",
    ];
    retryable_needles
        .iter()
        .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent::bridge::ProviderErrorKind;

    #[test]
    fn http_429_extracts_retry_after_seconds() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "5".parse().unwrap());

        let classification = classify_http_provider_failure(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            &headers,
            r#"{"error":{"code":"rate_limit_exceeded"}}"#,
        );

        assert_eq!(classification.kind, ProviderErrorKind::Retryable);
        assert_eq!(classification.http_status, Some(429));
        assert_eq!(
            classification.provider_code.as_deref(),
            Some("rate_limit_exceeded")
        );
        assert_eq!(classification.retry_after_ms, Some(5_000));
    }

    #[test]
    fn http_5xx_is_retryable() {
        let headers = reqwest::header::HeaderMap::new();
        let classification =
            classify_http_provider_failure(reqwest::StatusCode::BAD_GATEWAY, &headers, "");

        assert_eq!(classification.kind, ProviderErrorKind::Retryable);
        assert_eq!(classification.http_status, Some(502));
        assert_eq!(
            classification.provider_code.as_deref(),
            Some("provider_5xx")
        );
    }

    #[test]
    fn http_408_is_retryable_timeout() {
        let headers = reqwest::header::HeaderMap::new();
        let classification =
            classify_http_provider_failure(reqwest::StatusCode::REQUEST_TIMEOUT, &headers, "");

        assert_eq!(classification.kind, ProviderErrorKind::Retryable);
        assert_eq!(classification.http_status, Some(408));
        assert_eq!(classification.provider_code.as_deref(), Some("timeout"));
    }

    #[test]
    fn auth_status_is_fatal() {
        let headers = reqwest::header::HeaderMap::new();
        let classification =
            classify_http_provider_failure(reqwest::StatusCode::UNAUTHORIZED, &headers, "");

        assert_eq!(classification.kind, ProviderErrorKind::Fatal);
        assert_eq!(classification.http_status, Some(401));
        assert_eq!(classification.provider_code.as_deref(), Some("auth_error"));
    }

    #[test]
    fn usage_limit_body_keeps_429_fatal() {
        let headers = reqwest::header::HeaderMap::new();
        let classification = classify_http_provider_failure(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            &headers,
            r#"{"error":{"code":"usage_limit_reached"}}"#,
        );

        assert_eq!(classification.kind, ProviderErrorKind::Fatal);
        assert_eq!(classification.http_status, Some(429));
        assert_eq!(
            classification.provider_code.as_deref(),
            Some("usage_limit_reached")
        );
    }

    #[test]
    fn context_and_invalid_request_bodies_are_fatal() {
        let headers = reqwest::header::HeaderMap::new();
        let context = classify_http_provider_failure(
            reqwest::StatusCode::BAD_REQUEST,
            &headers,
            r#"{"error":{"code":"context_length_exceeded"}}"#,
        );
        let invalid = classify_http_provider_failure(
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
            &headers,
            r#"{"error":{"type":"invalid_request_error"}}"#,
        );

        assert_eq!(context.kind, ProviderErrorKind::Fatal);
        assert_eq!(
            context.provider_code.as_deref(),
            Some("context_length_exceeded")
        );
        assert_eq!(invalid.kind, ProviderErrorKind::Fatal);
        assert_eq!(
            invalid.provider_code.as_deref(),
            Some("invalid_request_error")
        );
    }

    #[test]
    fn rate_limit_reset_after_header_is_retry_delay() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ratelimit-reset-after", "1.25".parse().unwrap());

        let classification =
            classify_http_provider_failure(reqwest::StatusCode::TOO_MANY_REQUESTS, &headers, "");

        assert_eq!(classification.kind, ProviderErrorKind::Retryable);
        assert_eq!(classification.retry_after_ms, Some(1_250));
    }

    #[test]
    fn rate_limit_reset_epoch_header_is_retry_delay() {
        let mut headers = reqwest::header::HeaderMap::new();
        let reset_at = chrono::Utc::now().timestamp_millis() + 2_000;
        headers.insert("x-ratelimit-reset", reset_at.to_string().parse().unwrap());

        let classification =
            classify_http_provider_failure(reqwest::StatusCode::TOO_MANY_REQUESTS, &headers, "");

        let delay = classification
            .retry_after_ms
            .expect("x-ratelimit-reset should produce delay");
        assert!(delay <= 2_000);
    }
}
