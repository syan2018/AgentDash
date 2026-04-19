use std::sync::Arc;

/// 进程级不变配置 — 启动时从环境变量推导，整个生命周期内不变。
///
/// 通过 `SharedPlatformConfig` (`Arc<PlatformConfig>`) 在各层间共享，
/// 避免将单个字段逐层透传到 10+ 个结构体中。
#[derive(Debug, Clone)]
pub struct PlatformConfig {
    /// 平台内置 MCP server 基础 URL（如 `http://127.0.0.1:3001`）。
    /// `None` 时跳过所有平台 MCP 端点注入。
    pub mcp_base_url: Option<String>,
}

pub type SharedPlatformConfig = Arc<PlatformConfig>;
