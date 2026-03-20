use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::{Query, State},
    response::IntoResponse,
};
use futures::StreamExt;

use crate::app_state::AppState;

#[derive(Debug, serde::Deserialize)]
pub struct DiscoveredOptionsQuery {
    pub executor: String,
    pub variant: Option<String>,
    pub working_dir: Option<String>,
}

/// NDJSON 流：执行器发现选项（JSON Patch 增量推送）
///
/// 消息格式（每行一个 JSON 对象）：
///   {"Ready":true}
///   {"JsonPatch":[...]}   // 零到多条
///   {"finished":true}     // 正常结束
///   {"Error":"..."}       // 错误结束
pub async fn discovered_options_stream(
    State(state): State<Arc<AppState>>,
    Query(q): Query<DiscoveredOptionsQuery>,
) -> impl IntoResponse {
    let connector = state.services.connector.clone();

    let stream = async_stream::stream! {
        // 先发 Ready
        if let Some(line) = to_ndjson_line(&serde_json::json!({ "Ready": true })) {
            yield Ok::<Bytes, std::convert::Infallible>(line);
        }

        let working_dir = q
            .working_dir
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(|rel| std::env::current_dir().unwrap_or_default().join(rel));

        match connector.discover_options_stream(&q.executor, q.variant.as_deref(), working_dir).await {
            Ok(mut patches) => {
                while let Some(patch) = patches.next().await {
                    if let Some(line) = to_ndjson_line(&serde_json::json!({ "JsonPatch": patch })) {
                        yield Ok(line);
                    }
                }
                if let Some(line) = to_ndjson_line(&serde_json::json!({ "finished": true })) {
                    yield Ok(line);
                }
            }
            Err(e) => {
                if let Some(line) = to_ndjson_line(&serde_json::json!({ "Error": e.to_string() })) {
                    yield Ok(line);
                }
            }
        }
    };

    (
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/x-ndjson; charset=utf-8",
            ),
            (axum::http::header::CACHE_CONTROL, "no-cache, no-transform"),
            (axum::http::header::CONNECTION, "keep-alive"),
            (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from_stream(stream),
    )
}

fn to_ndjson_line(value: &serde_json::Value) -> Option<Bytes> {
    match serde_json::to_vec(value) {
        Ok(mut bytes) => {
            bytes.push(b'\n');
            Some(Bytes::from(bytes))
        }
        Err(err) => {
            tracing::error!(error = %err, "序列化 discovered_options NDJSON 消息失败");
            None
        }
    }
}
