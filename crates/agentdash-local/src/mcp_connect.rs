//! 本机 MCP transport 连接 helper。
//!
//! 连接池管理（`mcp_client_manager`）与一次性 probe（`handlers::mcp_relay`）都通过
//! streamable-http worker 连接 MCP server，此处集中 worker 构造，消除 reqwest client +
//! transport config 的逐字复制；握手与错误包装仍由各调用方按自身语境处理。

use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransportConfig, StreamableHttpClientWorker,
};
use std::collections::HashMap;

use agentdash_domain::mcp_preset::McpHttpHeader;
use reqwest::header::{HeaderName, HeaderValue};

/// 构造连接指定 URL 的 streamable-http MCP worker（默认 reqwest client）。
pub(crate) fn mcp_http_worker(
    url: &str,
    headers: &[McpHttpHeader],
) -> Result<StreamableHttpClientWorker<reqwest::Client>, anyhow::Error> {
    let config = StreamableHttpClientTransportConfig::with_uri(url.to_string())
        .custom_headers(build_header_map(headers)?);
    Ok(StreamableHttpClientWorker::new(
        reqwest::Client::new(),
        config,
    ))
}

fn build_header_map(
    headers: &[McpHttpHeader],
) -> Result<HashMap<HeaderName, HeaderValue>, anyhow::Error> {
    let mut map = HashMap::new();
    for header in headers {
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|error| anyhow::anyhow!("MCP HTTP header name 无效: {error}"))?;
        let value = HeaderValue::from_str(&header.value)
            .map_err(|error| anyhow::anyhow!("MCP HTTP header value 无效: {error}"))?;
        map.insert(name, value);
    }
    Ok(map)
}
