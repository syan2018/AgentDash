//! `rmcp` StreamableHttp implementation of the [`McpProbeTransport`] SPI port.
//!
//! Establishes a one-shot client connection to an HTTP/SSE MCP server and
//! returns the advertised tool list. Timeout / latency / result shaping are
//! owned by the application caller.

use agentdash_spi::platform::mcp_probe::{McpProbeTransport, McpProbedTool};
use async_trait::async_trait;
use rmcp::{
    ServiceExt,
    transport::streamable_http_client::{
        StreamableHttpClientTransportConfig, StreamableHttpClientWorker,
    },
};

/// MCP probe transport backed by the `rmcp` StreamableHttp client.
#[derive(Debug, Default, Clone)]
pub struct RmcpProbeTransport;

impl RmcpProbeTransport {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl McpProbeTransport for RmcpProbeTransport {
    async fn probe_http(&self, url: &str) -> Result<Vec<McpProbedTool>, String> {
        let worker = StreamableHttpClientWorker::new(
            reqwest::Client::new(),
            StreamableHttpClientTransportConfig::with_uri(url.to_string()),
        );
        let client = ()
            .serve(worker)
            .await
            .map_err(|e| format!("连接 MCP Server 失败: {e}"))?;
        let tools = client
            .list_all_tools()
            .await
            .map_err(|e| format!("list_tools 失败: {e}"))?;
        let _ = client.cancel().await;

        Ok(tools
            .into_iter()
            .map(|t| McpProbedTool {
                name: t.name.to_string(),
                description: t.description.as_deref().unwrap_or("").to_string(),
            })
            .collect())
    }
}
