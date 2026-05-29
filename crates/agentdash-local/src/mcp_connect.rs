//! 本机 MCP transport 连接 helper。
//!
//! 连接池管理（`mcp_client_manager`）与一次性 probe（`handlers::mcp_relay`）都通过
//! streamable-http worker 连接 MCP server，此处集中 worker 构造，消除 reqwest client +
//! transport config 的逐字复制；握手与错误包装仍由各调用方按自身语境处理。

use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransportConfig, StreamableHttpClientWorker,
};

/// 构造连接指定 URL 的 streamable-http MCP worker（默认 reqwest client）。
pub(crate) fn mcp_http_worker(url: &str) -> StreamableHttpClientWorker<reqwest::Client> {
    StreamableHttpClientWorker::new(
        reqwest::Client::new(),
        StreamableHttpClientTransportConfig::with_uri(url.to_string()),
    )
}
