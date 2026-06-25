//! Session launch envelope port 的 API 层薄委托。
//!
//! 所有 compose 逻辑由 application 层的 `FrameConstructionService` 承接，此文件仅保留：
//! 1. `AppStateFrameLaunchEnvelopePort` — 实现 port 的薄委托层
//! 2. test-only 的 API error encode/decode 辅助（供集成测试使用）

use std::sync::Arc;

use async_trait::async_trait;

use agentdash_application::frame_construction::{FrameConstructionDeps, FrameConstructionService};
use agentdash_application_lifecycle::AgentRunLifecycleSurfaceProjector;
use agentdash_application_ports::frame_launch_envelope::{
    FrameLaunchEnvelope, FrameLaunchEnvelopePort, FrameLaunchEnvelopeRequest,
};
use agentdash_spi::ConnectorError;

use crate::app_state::AppState;
#[cfg(test)]
use crate::rpc::ApiError;

/// 使用 `Arc<AppState>` 的主通道 launch envelope provider。
///
/// 内部持有 `FrameConstructionService`（application 层），将所有 compose 路由
/// 和 frame 持久化委托给该 service，自身不再包含任何业务分支。
pub struct AppStateFrameLaunchEnvelopePort {
    service: FrameConstructionService,
}

impl AppStateFrameLaunchEnvelopePort {
    pub fn new(state: Arc<AppState>) -> Self {
        let service = FrameConstructionService::new(FrameConstructionDeps {
            repos: state.repos.clone(),
            vfs_service: state.services.vfs_service.clone(),
            availability: state.services.backend_registry.clone(),
            platform_config: state.config.platform_config.clone(),
            audit_bus: state.services.audit_bus.clone(),
            companion_facts: Arc::new(state.services.session_runtime_transition.clone()),
            lifecycle_surface_projection: Arc::new(AgentRunLifecycleSurfaceProjector::new(
                &state.repos.to_lifecycle_repository_set(),
            )),
            connector: state.services.connector.clone(),
            extra_skill_dirs: state.services.extra_skill_dirs.clone(),
            skill_discovery_providers: state.services.skill_discovery_providers.clone(),
            memory_discovery_providers: state.services.memory_discovery_providers.clone(),
        });
        Self { service }
    }
}

#[async_trait]
impl FrameLaunchEnvelopePort for AppStateFrameLaunchEnvelopePort {
    async fn build_launch_envelope(
        &self,
        input: FrameLaunchEnvelopeRequest,
    ) -> Result<FrameLaunchEnvelope, ConnectorError> {
        self.service
            .construct_launch_envelope_from_request(input)
            .await
            .map(|envelope| envelope.into_runtime_session_launch_envelope())
    }
}

// ─── Test-only API error tunneling ───

#[cfg(test)]
const CONSTRUCTION_API_ERROR_PREFIX: &str = "__construction_api_error__:";

#[cfg(test)]
fn encode_api_error(kind: &str, message: String) -> String {
    format!("{CONSTRUCTION_API_ERROR_PREFIX}{kind}:{message}")
}

#[cfg(test)]
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
        "internal" => {
            tracing::error!(detail, "frame launch envelope internal error");
            Some(ApiError::Internal(String::from("内部 session 构建错误")))
        }
        _ => None,
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
