//! 会话标题自动生成
//!
//! 定义 `SessionTitleGenerator` trait 供外部注入 LLM 实现。
//! launch execution 在首轮 prompt accepted 后异步触发标题生成任务。

use async_trait::async_trait;

/// 标题生成抽象 — 由服务启动时注入具体 LLM 实现。
#[async_trait]
pub trait SessionTitleGenerator: Send + Sync {
    /// 根据用户首轮 prompt 文本生成简短会话标题。
    /// 返回 Ok(title) 或 Err(原因) — 失败时调用方保留原标题。
    async fn generate_title(&self, user_prompt: &str) -> Result<String, String>;
}
