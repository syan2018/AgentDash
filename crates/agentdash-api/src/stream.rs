use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;

use agentdash_state::events::StreamEvent;
use agentdash_state::models::StateChange;

use crate::app_state::AppState;
use crate::rpc::ApiError;

/// NDJSON 事件流端点
///
/// 客户端通过 SSE 连接接收实时状态变更。
/// 虽然 PRD 提到纯 NDJSON，但 SSE 提供更好的浏览器兼容性，
/// 且语义上等价于 NDJSON 流（每个 data 字段是一行 JSON）。
pub async fn event_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let mut rx = state.subscribe_events();

    let stream = async_stream::stream! {
        let last_id = state.store.latest_event_id().await.unwrap_or(0);
        let connected = StreamEvent::Connected { last_event_id: last_id };
        if let Ok(json) = serde_json::to_string(&connected) {
            yield Ok(Event::default().data(json));
        }

        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        yield Ok(Event::default().data(json));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("事件流落后 {} 条消息", n);
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Resume 端点 — 获取指定 ID 之后的状态变更
///
/// 客户端在断连重连后，使用最后收到的 event_id 请求增量数据。
pub async fn get_events_since(
    State(state): State<Arc<AppState>>,
    Path(since_id): Path<i64>,
) -> Result<Json<Vec<StateChange>>, ApiError> {
    let changes = state.store.get_changes_since(since_id, 1000).await?;
    Ok(Json(changes))
}
