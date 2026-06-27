use std::sync::Arc;

/// Process-level immutable configuration consumed by lifecycle orchestration.
#[derive(Debug, Clone)]
pub struct PlatformConfig {
    /// Base URL of the platform-bundled MCP server.
    pub mcp_base_url: Option<String>,
}

pub type SharedPlatformConfig = Arc<PlatformConfig>;
