import { api } from "../api/client";
import type {
  CloneMcpPresetRequest,
  CreateMcpPresetRequest,
  ListMcpPresetQuery,
  McpPresetDto,
  McpProbeTarget,
  McpRoutePolicy,
  McpRuntimeBindingConfig,
  McpTransportConfig,
  ProbeMcpPresetRequest,
  ProbeMcpPresetResponse,
  UpdateMcpPresetRequest,
} from "../types";

export const DEFAULT_MCP_PROBE_TARGET: McpProbeTarget = { kind: "default_user_local" };

export async function fetchProjectMcpPresets(
  projectId: string,
  query?: ListMcpPresetQuery,
): Promise<McpPresetDto[]> {
  const params = new URLSearchParams();
  if (query?.source) {
    params.set("source", query.source);
  }
  const qs = params.toString() ? `?${params}` : "";
  return await api.get<McpPresetDto[]>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets${qs}`,
  );
}

export async function createMcpPreset(
  projectId: string,
  input: CreateMcpPresetRequest,
): Promise<McpPresetDto> {
  return await api.post<McpPresetDto>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets`,
    input,
  );
}

export async function fetchMcpPreset(
  projectId: string,
  presetId: string,
): Promise<McpPresetDto> {
  return await api.get<McpPresetDto>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets/${encodeURIComponent(presetId)}`,
  );
}

export async function updateMcpPreset(
  projectId: string,
  presetId: string,
  input: UpdateMcpPresetRequest,
): Promise<McpPresetDto> {
  return await api.patch<McpPresetDto>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets/${encodeURIComponent(presetId)}`,
    input,
  );
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
  return await api.post<McpPresetDto>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets/${encodeURIComponent(presetId)}/clone`,
    input,
  );
}

/**
 * 触发 MCP transport 的 probe —— 临时连接 MCP Server 获取工具列表和连通性状态。
 *
 * 不绑定已落库的 Preset：调用方直接把当前要验证的 transport 配置传进来
 * （卡片用已保存的、detail dialog 用编辑中的），确保"所见即所测"。
 *
 * 后端约束：15 秒超时；route_policy=relay 或 stdio/auto 通过本机 relay 探测，其余使用 URL 直连探测。
 * 响应体形状为 tagged union（`status` 为 discriminator），展示层通过 probe view model 解释。
 */
export async function probeMcpTransport(
  projectId: string,
  transport: McpTransportConfig,
  routePolicy: McpRoutePolicy,
  runtimeBinding?: McpRuntimeBindingConfig | null,
  probeTarget?: McpProbeTarget | null,
): Promise<ProbeMcpPresetResponse> {
  const input: ProbeMcpPresetRequest = {
    transport,
    route_policy: routePolicy,
    probe_target: probeTarget ?? DEFAULT_MCP_PROBE_TARGET,
  };
  if (runtimeBinding) {
    input.runtime_binding = runtimeBinding;
  }
  return await api.post<ProbeMcpPresetResponse>(
    `/projects/${encodeURIComponent(projectId)}/mcp-presets/probe`,
    input,
  );
}
