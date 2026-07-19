//! SPI port for probing remote MCP servers over HTTP/SSE.
//!
//! The concrete MCP client transport (currently `rmcp` StreamableHttp) lives
//! in infrastructure. Application owns the transport dispatch (HTTP vs relay),
//! timeout, latency measurement, and result shaping — it depends only on this
//! port for the actual network round-trip.

use async_trait::async_trait;

use crate::McpHttpHeader;

/// A single tool discovered while probing an MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpProbedTool {
    pub name: String,
    pub description: String,
}

/// Establishes a one-shot connection to an HTTP/SSE MCP server and returns its
/// advertised tool list. Implementations must not impose their own timeout;
/// the caller wraps the call to bound the whole probe.
#[async_trait]
pub trait McpProbeTransport: Send + Sync {
    async fn probe_http(
        &self,
        url: &str,
        headers: &[McpHttpHeader],
    ) -> Result<Vec<McpProbedTool>, String>;
}
