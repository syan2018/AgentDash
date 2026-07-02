import type {
  McpRoutePolicy,
  McpTransportConfig,
  ProbeMcpPresetResponse,
  ProbeMcpToolInfo,
  ToolDescriptor,
} from "../../types";

export type McpProbeViewStatus = "idle" | "ok" | "error" | "unsupported";
export type McpProbeTone = "success" | "danger" | "muted";

export interface McpProbeViewModel {
  status: McpProbeViewStatus;
  tools: ReadonlyArray<ProbeMcpToolInfo>;
  toolCount: number;
  latencyMs: number | null;
  headerLabel: string;
  bodyMessage: string;
  bodyTitle: string | null;
  bodyTone: McpProbeTone;
  detailMessage: string | null;
  detailTone: McpProbeTone;
  descriptorMessage: string | null;
  showToolGrid: boolean;
}

export interface McpProbeToolDescriptorInput {
  capabilityKey: string;
  serverName: string;
  result: ProbeMcpPresetResponse;
}

export function buildMcpProbeViewModel(
  result: ProbeMcpPresetResponse | null,
): McpProbeViewModel {
  if (result === null) {
    return {
      status: "idle",
      tools: [],
      toolCount: 0,
      latencyMs: null,
      headerLabel: "可用工具",
      bodyMessage: "尚未探测",
      bodyTitle: null,
      bodyTone: "muted",
      detailMessage: null,
      detailTone: "muted",
      descriptorMessage: null,
      showToolGrid: false,
    };
  }

  if (result.status === "ok") {
    const toolCount = result.tools.length;
    return {
      status: "ok",
      tools: result.tools,
      toolCount,
      latencyMs: result.latency_ms,
      headerLabel: `发现 ${toolCount} 个工具（${result.latency_ms} ms）`,
      bodyMessage: toolCount > 0 ? "" : "（未返回工具）",
      bodyTitle: null,
      bodyTone: toolCount > 0 ? "success" : "muted",
      detailMessage: `✓ 连接成功（${result.latency_ms} ms）· ${
        toolCount > 0 ? `发现 ${toolCount} 个工具` : "未返回工具"
      }`,
      detailTone: "success",
      descriptorMessage: toolCount > 0 ? null : "MCP Server 未返回任何工具",
      showToolGrid: toolCount > 0,
    };
  }

  if (result.status === "error") {
    return {
      status: "error",
      tools: [],
      toolCount: 0,
      latencyMs: null,
      headerLabel: "可用工具",
      bodyMessage: `✗ ${truncateProbeMessage(result.error)}`,
      bodyTitle: result.error,
      bodyTone: "danger",
      detailMessage: `✗ 探测失败：${result.error}`,
      detailTone: "danger",
      descriptorMessage: `探测失败：${result.error}`,
      showToolGrid: false,
    };
  }

  return {
    status: "unsupported",
    tools: [],
    toolCount: 0,
    latencyMs: null,
    headerLabel: "可用工具",
    bodyMessage: `⚠ ${result.reason}`,
    bodyTitle: result.reason,
    bodyTone: "muted",
    detailMessage: `⚠ ${result.reason}`,
    detailTone: "muted",
    descriptorMessage: result.reason,
    showToolGrid: false,
  };
}

export function mapMcpProbeToToolDescriptors({
  capabilityKey,
  serverName,
  result,
}: McpProbeToolDescriptorInput): ToolDescriptor[] {
  const view = buildMcpProbeViewModel(result);
  if (view.status === "ok" && view.showToolGrid) {
    return view.tools.map((tool): ToolDescriptor => ({
      name: tool.name,
      display_name: tool.name,
      description: tool.description || `MCP 工具 ${serverName}/${tool.name}`,
      source: { type: "mcp", server_name: serverName },
      capability_key: capabilityKey,
    }));
  }

  return [
    mcpProbePlaceholderDescriptor(
      capabilityKey,
      serverName,
      view.descriptorMessage ?? "MCP Server 未返回任何工具",
    ),
  ];
}

export function mcpProbePlaceholderDescriptor(
  capabilityKey: string,
  serverName: string,
  description: string,
): ToolDescriptor {
  return {
    name: `mcp:${serverName}`,
    display_name: `MCP: ${serverName}`,
    description,
    source: { type: "mcp", server_name: serverName },
    capability_key: capabilityKey,
  };
}

export function describeMcpProbeTransport(
  transportType: McpTransportConfig["type"],
  routePolicy: McpRoutePolicy,
): string {
  if (routePolicy === "relay" || (routePolicy === "auto" && transportType === "stdio")) {
    return "通过本机 relay 连接 MCP Server 并调用 tools/list；15 秒超时";
  }
  return "实时连接 MCP Server 并调用 tools/list；15 秒超时";
}

function truncateProbeMessage(message: string): string {
  return message.length > 80 ? `${message.slice(0, 80)}…` : message;
}
