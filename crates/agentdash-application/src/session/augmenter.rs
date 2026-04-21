//! Prompt request 增强契约。
//!
//! Session 的主通道（用户 HTTP prompt）和 auto-resume 通道都必须通过同一份
//! 增强逻辑才能拿到 owner context / MCP server 绑定 / flow capabilities /
//! system context 等运行时字段，否则会出现"通道漂移"——auto-resume 拿到
//! 的是一个"裸" PromptSessionRequest，Agent 丢失工作流背景后容易复读。
//!
//! API 层实现此 trait，在 AppState 初始化时通过 `SessionHub::set_prompt_augmenter`
//! 注入。SessionHub 在内部需要构造 PromptSessionRequest（例如 hook auto-resume）
//! 时一律先经过 augmenter，与 HTTP 主通道对齐。

use std::sync::Arc;

use agentdash_spi::ConnectorError;
use async_trait::async_trait;

use super::types::PromptSessionRequest;

/// 用于把"裸" `PromptSessionRequest` 增强成与主通道一致的完整请求。
#[async_trait]
pub trait PromptRequestAugmenter: Send + Sync {
    /// 依据 session 的 owner binding / workspace / agent preset / workflow 等信息，
    /// 补齐 `PromptSessionRequest` 的后端注入字段（mcp_servers / vfs / flow_capabilities
    /// / system_context / bootstrap_action / effective_capability_keys 等）。
    async fn augment(
        &self,
        session_id: &str,
        req: PromptSessionRequest,
    ) -> Result<PromptSessionRequest, ConnectorError>;
}

/// 动态类型别名，便于在 hub 内存储。
pub type SharedPromptRequestAugmenter = Arc<dyn PromptRequestAugmenter>;
