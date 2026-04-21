//! `PromptRequestAugmenter` 的 API 层实现。
//!
//! 把"裸" `PromptSessionRequest` 代入与 HTTP 主通道同一条 `augment_prompt_request_for_owner`
//! 路径，使 hub auto-resume 与用户手动 prompt 完全对齐。
//!
//! 为什么放这里：augment 逻辑依赖 `Arc<AppState>`（repos、services、platform_config），
//! 这些都是 API 层构造的；把 trait impl 也放在 API 层最自然，也不必把依赖下沉到
//! application crate。

use std::sync::Arc;

use async_trait::async_trait;

use agentdash_application::session::{PromptRequestAugmenter, PromptSessionRequest};
use agentdash_spi::ConnectorError;

use crate::app_state::AppState;
use crate::routes::acp_sessions::augment_prompt_request_for_owner;
use crate::rpc::ApiError;

/// 使用 `Arc<AppState>` 的主通道增强器。在 AppState 初始化完成后通过
/// `SessionHub::set_prompt_augmenter` 注入。
pub struct AppStatePromptAugmenter {
    state: Arc<AppState>,
}

impl AppStatePromptAugmenter {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

fn api_error_to_connector(error: ApiError) -> ConnectorError {
    match error {
        ApiError::BadRequest(msg) => ConnectorError::InvalidConfig(msg),
        ApiError::Unauthorized(msg)
        | ApiError::Forbidden(msg)
        | ApiError::NotFound(msg)
        | ApiError::Conflict(msg)
        | ApiError::UnprocessableEntity(msg)
        | ApiError::ServiceUnavailable(msg)
        | ApiError::Internal(msg) => ConnectorError::Runtime(msg),
    }
}

#[async_trait]
impl PromptRequestAugmenter for AppStatePromptAugmenter {
    async fn augment(
        &self,
        session_id: &str,
        req: PromptSessionRequest,
    ) -> Result<PromptSessionRequest, ConnectorError> {
        augment_prompt_request_for_owner(&self.state, session_id, req)
            .await
            .map_err(api_error_to_connector)
    }
}
