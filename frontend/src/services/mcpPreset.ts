import { api } from "../api/client";
import { asRecord } from "../api/mappers";
import type {
  BootstrapMcpPresetRequest,
  CloneMcpPresetRequest,
  CreateMcpPresetRequest,
  ListMcpPresetQuery,
  McpPresetDto,
  McpPresetSource,
  McpServerDecl,
  UpdateMcpPresetRequest,
} from "../types";

// ─── Mapper ──────────────────────────────────────────
//
// 仅做 `unknown → typed object` + 状态值归一化，不做字段名转换。
// 严格遵循后端 DTO 的 snake_case；source 字段做白名单收窄。

function normalizeSource(value: unknown): McpPresetSource {
  if (value === "builtin") return "builtin";
  // 默认回退到 user —— 后端只会产出 builtin / user 两种值，别的值视为脏数据
  return "user";
}

/**
 * 把后端返回的 `server_decl` 归一化为前端 `McpServerDecl`。
 *
 * 对齐 `frontend/src/types/index.ts` 中的联合体定义（type: http | sse | stdio）。
 * 未识别 type 时抛错——比起长期兼容更推荐在 mapper 里暴露出契约漂移。
 */
function mapMcpServerDecl(raw: unknown): McpServerDecl {
  const value = asRecord(raw);
  if (!value) {
    throw new Error("mcp preset server_decl 缺失或不是对象");
  }
  const type = value.type;
  switch (type) {
    case "http":
    case "sse":
    case "stdio":
      // 直接返回——TS 类型守卫由联合体 `type` 字面量承担
      return value as unknown as McpServerDecl;
    default:
      throw new Error(`未知的 mcp preset server_decl.type: ${String(type)}`);
  }
}

export function mapMcpPreset(raw: Record<string, unknown>): McpPresetDto {
  return {
    id: String(raw.id ?? ""),
    project_id: String(raw.project_id ?? ""),
    name: String(raw.name ?? ""),
    description:
      raw.description === null || raw.description === undefined
        ? null
        : String(raw.description),
    server_decl: mapMcpServerDecl(raw.server_decl),
    source: normalizeSource(raw.source),
    builtin_key:
      raw.builtin_key === null || raw.builtin_key === undefined
        ? null
        : String(raw.builtin_key),
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
}

// ─── API client ──────────────────────────────────────
//
// 路由前缀：/projects/:project_id/mcp-presets
// 对齐 crates/agentdash-api/src/routes.rs:166-184

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
