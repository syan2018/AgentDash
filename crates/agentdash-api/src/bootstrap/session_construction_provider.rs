//! `SessionConstructionProvider` 的 API 层实现。
//!
//! 把原始 prompt 输入代入与 HTTP 主通道同一条 `build_session_construction_for_launch`
//! 路径，使 hub auto-resume 与用户手动 prompt 完全对齐。
//!
//! 为什么放这里：construction 逻辑依赖 `Arc<AppState>`（repos、services、platform_config），
//! 这些都是 API 层构造的；把 trait impl 也放在 API 层最自然，也不必把依赖下沉到
//! application crate。

use std::sync::Arc;

use async_trait::async_trait;

use agentdash_application::session::construction::SessionConstructionFacts;
use agentdash_application::session::{LaunchCommand, SessionConstructionProvider, UserPromptInput};
use agentdash_spi::ConnectorError;

use crate::app_state::AppState;
use crate::bootstrap::session_construction_bootstrap::build_session_construction_for_launch;
use crate::rpc::ApiError;

/// 使用 `Arc<AppState>` 的主通道 construction provider。在 AppState 初始化完成后通过
/// `SessionHub::set_session_construction_provider` 注入。
pub struct AppStateSessionConstructionProvider {
    state: Arc<AppState>,
}

impl AppStateSessionConstructionProvider {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

const CONSTRUCTION_API_ERROR_PREFIX: &str = "__construction_api_error__:";

fn encode_api_error(kind: &str, message: String) -> String {
    format!("{CONSTRUCTION_API_ERROR_PREFIX}{kind}:{message}")
}

pub(crate) fn decode_construction_runtime_error(message: &str) -> Option<ApiError> {
    let payload = message.strip_prefix(CONSTRUCTION_API_ERROR_PREFIX)?;
    let (kind, detail) = payload.split_once(':')?;
    match kind {
        "unauthorized" => Some(ApiError::Unauthorized(detail.to_string())),
        "forbidden" => Some(ApiError::Forbidden(detail.to_string())),
        "not_found" => Some(ApiError::NotFound(detail.to_string())),
        "conflict" => Some(ApiError::Conflict(detail.to_string())),
        "unprocessable_entity" => Some(ApiError::UnprocessableEntity(detail.to_string())),
        "service_unavailable" => Some(ApiError::ServiceUnavailable(detail.to_string())),
        "internal" => Some(ApiError::Internal(detail.to_string())),
        _ => None,
    }
}

fn api_error_to_connector(error: ApiError) -> ConnectorError {
    match error {
        ApiError::BadRequest(msg) => ConnectorError::InvalidConfig(msg),
        ApiError::Unauthorized(msg) => {
            ConnectorError::Runtime(encode_api_error("unauthorized", msg))
        }
        ApiError::Forbidden(msg) => ConnectorError::Runtime(encode_api_error("forbidden", msg)),
        ApiError::NotFound(msg) => ConnectorError::Runtime(encode_api_error("not_found", msg)),
        ApiError::Conflict(msg) => ConnectorError::Runtime(encode_api_error("conflict", msg)),
        ApiError::UnprocessableEntity(msg) => {
            ConnectorError::Runtime(encode_api_error("unprocessable_entity", msg))
        }
        ApiError::ServiceUnavailable(msg) => {
            ConnectorError::Runtime(encode_api_error("service_unavailable", msg))
        }
        ApiError::Internal(msg) => ConnectorError::Runtime(encode_api_error("internal", msg)),
    }
}

#[async_trait]
impl SessionConstructionProvider for AppStateSessionConstructionProvider {
    async fn build_construction(
        &self,
        session_id: &str,
        command: &LaunchCommand,
    ) -> Result<(UserPromptInput, SessionConstructionFacts), ConnectorError> {
        build_session_construction_for_launch(
            &self.state,
            session_id,
            command.user_input().clone(),
            SessionConstructionFacts::default(),
            command.task_hint(),
            command.companion_hint(),
            command.local_relay_mcp_declarations().to_vec(),
        )
        .await
        .map_err(api_error_to_connector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_construction_runtime_error_roundtrip_not_found() {
        let encoded = encode_api_error("not_found", "session missing".to_string());
        let decoded = decode_construction_runtime_error(&encoded);
        match decoded {
            Some(ApiError::NotFound(message)) => assert_eq!(message, "session missing"),
            other => panic!("期望 NotFound，实际为: {other:?}"),
        }
    }

    #[test]
    fn decode_construction_runtime_error_ignores_plain_runtime_text() {
        assert!(decode_construction_runtime_error("plain runtime error").is_none());
    }
}
