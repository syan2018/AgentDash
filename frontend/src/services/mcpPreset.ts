import { api } from "../api/client";
import { asRecord } from "../api/mappers";
import type {
  BootstrapMcpPresetRequest,
  CloneMcpPresetRequest,
  CreateMcpPresetRequest,
  ListMcpPresetQuery,
  McpPresetDto,
  McpPresetSource,
  McpRoutePolicy,
  McpTransportConfig,
  ProbeMcpPresetResponse,
  UpdateMcpPresetRequest,
} from "../types";

function normalizeSource(value: unknown): McpPresetSource {
  if (value === "builtin") return "builtin";
  return "user";
}

function normalizeRoutePolicy(value: unknown): McpRoutePolicy {
  if (value === "relay" || value === "direct") return value;
  return "auto";
}

function mapMcpTransport(raw: unknown): McpTransportConfig {
  const value = asRecord(raw);
  if (!value) {
    throw new Error("mcp preset transport 缺失或不是对象");
  }
  switch (value.type) {
    case "http":
    case "sse":
    case "stdio":
      return value as unknown as McpTransportConfig;
    default:
      throw new Error(`未知的 mcp preset transport.type: ${String(value.type)}`);
  }
}

export function mapMcpPreset(raw: Record<string, unknown>): McpPresetDto {
  return {
    id: String(raw.id ?? ""),
    project_id: String(raw.project_id ?? ""),
    key: String(raw.key ?? ""),
    display_name: String(raw.display_name ?? raw.key ?? ""),
    description:
      raw.description === null || raw.description === undefined
        ? null
        : String(raw.description),
    transport: mapMcpTransport(raw.transport),
    route_policy: normalizeRoutePolicy(raw.route_policy),
    source: normalizeSource(raw.source),
    builtin_key:
      raw.builtin_key === null || raw.builtin_key === undefined
        ? null
        : String(raw.builtin_key),
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
}

export async function fetchProjectMcpPresets(
  projectId: string,
  query?: ListMcpPresetQuery,
): Promise<McpPresetDto[]> {
  const params = new URLSearchParams();
  if (query?.source) {
    params.set("source", query.source);
  }
  const qs = params.toString() ? `?${params}` : "";
  const raw = await api.get<Record<string, unknown>[]>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets${qs}`,
  );
  return raw.map(mapMcpPreset);
}

export async function createMcpPreset(
  projectId: string,
  input: CreateMcpPresetRequest,
): Promise<McpPresetDto> {
  const raw = await api.post<Record<string, unknown>>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets`,
    input,
  );
  return mapMcpPreset(raw);
}

export async function fetchMcpPreset(
  projectId: string,
  presetId: string,
): Promise<McpPresetDto> {
  const raw = await api.get<Record<string, unknown>>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets/${encodeURIComponent(presetId)}`,
  );
  return mapMcpPreset(raw);
}

export async function updateMcpPreset(
  projectId: string,
  presetId: string,
  input: UpdateMcpPresetRequest,
): Promise<McpPresetDto> {
  const raw = await api.patch<Record<string, unknown>>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets/${encodeURIComponent(presetId)}`,
    input,
  );
  return mapMcpPreset(raw);
}

export async function deleteMcpPreset(
  projectId: string,
  presetId: string,
): Promise<void> {
  await api.delete(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets/${encodeURIComponent(presetId)}`,
  );
}

export async function cloneMcpPreset(
  projectId: string,
  presetId: string,
  input: CloneMcpPresetRequest = {},
): Promise<McpPresetDto> {
  const raw = await api.post<Record<string, unknown>>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets/${encodeURIComponent(presetId)}/clone`,
    input,
  );
  return mapMcpPreset(raw);
}

export async function bootstrapMcpPresets(
  projectId: string,
  input: BootstrapMcpPresetRequest = {},
): Promise<McpPresetDto[]> {
  const raw = await api.post<Record<string, unknown>[]>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets/bootstrap`,
    input,
  );
  return raw.map(mapMcpPreset);
}

/**
 * 触发 MCP transport 的 probe —— 临时连接 MCP Server 获取工具列表和连通性状态。
 *
 * 不绑定已落库的 Preset：调用方直接把当前要验证的 transport 配置传进来
 * （卡片用已保存的、detail dialog 用编辑中的），确保"所见即所测"。
 *
 * 后端约束：15 秒超时；Stdio transport 返回 `unsupported`；Http/Sse 直连探测。
 * 响应体形状为 tagged union（`status` 为 discriminator），前端需按 status 分支处理。
 */
export async function probeMcpTransport(
  projectId: string,
  transport: McpTransportConfig,
): Promise<ProbeMcpPresetResponse> {
  return await api.post<ProbeMcpPresetResponse>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets/probe`,
    transport,
  );
}
