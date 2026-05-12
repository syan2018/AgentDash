import { api, authenticatedFetch } from "../api/client";
import { buildApiPath } from "../api/origin";
import { asRecord } from "../api/mappers";
import type {
  BootstrapSkillAssetRequest,
  CreateSkillAssetRequest,
  ListSkillAssetQuery,
  SkillAssetDto,
  SkillAssetFileDto,
  SkillAssetSource,
  UpdateSkillAssetRequest,
} from "../types";

export interface SkillAssetExtraFile {
  relative_path: string;
  content: string;
}

export interface SkillAssetDraft {
  id?: string;
  key: string;
  display_name: string;
  description: string;
  body: string;
  disable_model_invocation: boolean;
  files: SkillAssetExtraFile[];
  source?: SkillAssetSource;
  builtin_key?: string | null;
}

export interface SkillAssetValidationResult {
  ok: boolean;
  message?: string;
}

interface ParsedSkillMarkdown {
  name: string | null;
  description: string | null;
  disable_model_invocation: boolean;
  body: string;
}

function normalizeSource(value: unknown): SkillAssetSource {
  return value === "builtin_seed" ? "builtin_seed" : "user";
}

function mapSkillAssetFile(raw: unknown): SkillAssetFileDto {
  const value = asRecord(raw);
  if (!value) {
    throw new Error("skill asset file 缺失或不是对象");
  }
  return {
    path: String(value.path ?? ""),
    content: String(value.content ?? ""),
    kind: value.kind == null ? null : String(value.kind),
  };
}

export function mapSkillAsset(raw: Record<string, unknown>): SkillAssetDto {
  const files = Array.isArray(raw.files) ? raw.files.map(mapSkillAssetFile) : [];
  return {
    id: String(raw.id ?? ""),
    project_id: String(raw.project_id ?? ""),
    key: String(raw.key ?? ""),
    display_name: String(raw.display_name ?? raw.key ?? ""),
    description: String(raw.description ?? ""),
    source: normalizeSource(raw.source),
    builtin_key:
      raw.builtin_key === null || raw.builtin_key === undefined
        ? null
        : String(raw.builtin_key),
    disable_model_invocation: Boolean(raw.disable_model_invocation),
    files,
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
}

function parseFrontmatterValue(value: string): string {
  const trimmed = value.trim();
  if (trimmed.startsWith('"') && trimmed.endsWith('"')) {
    try {
      return JSON.parse(trimmed) as string;
    } catch {
      return trimmed.slice(1, -1);
    }
  }
  if (trimmed.startsWith("'") && trimmed.endsWith("'")) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

function parseSkillMarkdown(content: string): ParsedSkillMarkdown {
  const normalized = content.trimStart();
  if (!normalized.startsWith("---")) {
    return {
      name: null,
      description: null,
      disable_model_invocation: false,
      body: content,
    };
  }

  const closeIndex = normalized.slice(3).indexOf("\n---");
  if (closeIndex < 0) {
    return {
      name: null,
      description: null,
      disable_model_invocation: false,
      body: content,
    };
  }

  const frontmatter = normalized.slice(3, closeIndex + 3);
  const body = normalized.slice(closeIndex + 7).replace(/^\r?\n/, "");
  let name: string | null = null;
  let description: string | null = null;
  let disableModelInvocation = false;

  for (const line of frontmatter.split(/\r?\n/)) {
    const match = /^([A-Za-z0-9_-]+):\s*(.*)$/.exec(line.trim());
    if (!match) continue;
    const [, key, rawValue] = match;
    const value = parseFrontmatterValue(rawValue);
    if (key === "name") {
      name = value;
    } else if (key === "description") {
      description = value;
    } else if (key === "disable-model-invocation") {
      disableModelInvocation = value === "true";
    }
  }

  return {
    name,
    description,
    disable_model_invocation: disableModelInvocation,
    body,
  };
}

function quotedYamlString(value: string): string {
  return JSON.stringify(value);
}

export function buildSkillMarkdown(asset: SkillAssetDraft): string {
  const frontmatter = [
    "---",
    `name: ${asset.key.trim()}`,
    `description: ${quotedYamlString(asset.description.trim())}`,
    ...(asset.disable_model_invocation ? ["disable-model-invocation: true"] : []),
    "---",
  ].join("\n");
  const body = asset.body.trimEnd();
  return body ? `${frontmatter}\n${body}\n` : `${frontmatter}\n`;
}

export function draftFromSkillAsset(asset: SkillAssetDto): SkillAssetDraft {
  const skillFile = asset.files.find((file) => file.path === "SKILL.md");
  const parsed = skillFile ? parseSkillMarkdown(skillFile.content) : null;
  return {
    id: asset.id,
    key: parsed?.name?.trim() || asset.key,
    display_name: asset.display_name,
    description: parsed?.description?.trim() || asset.description,
    body: parsed?.body ?? "# 使用说明\n",
    disable_model_invocation:
      parsed?.disable_model_invocation ?? asset.disable_model_invocation,
    files: asset.files
      .filter((file) => file.path !== "SKILL.md")
      .map((file) => ({
        relative_path: file.path,
        content: file.content,
      }))
      .sort((a, b) => a.relative_path.localeCompare(b.relative_path, "zh-CN")),
    source: asset.source,
    builtin_key: asset.builtin_key,
  };
}

export function dtoFilesFromDraft(draft: SkillAssetDraft): SkillAssetFileDto[] {
  const normalized: SkillAssetDraft = {
    ...draft,
    key: draft.key.trim(),
    description: draft.description.trim(),
  };
  return [
    {
      path: "SKILL.md",
      content: buildSkillMarkdown(normalized),
    },
    ...normalized.files
      .filter((file) => normalizeSkillExtraPath(file.relative_path))
      .map((file) => ({
        path: normalizeSkillExtraPath(file.relative_path),
        content: file.content,
      })),
  ];
}

export function validateSkillName(name: string): SkillAssetValidationResult {
  const trimmed = name.trim();
  if (!trimmed) {
    return { ok: false, message: "Skill key 不能为空" };
  }
  if (trimmed.length > 64) {
    return { ok: false, message: "Skill key 不能超过 64 个字符" };
  }
  if (!/^[a-z0-9-]+$/.test(trimmed)) {
    return { ok: false, message: "Skill key 只能包含小写字母、数字和连字符" };
  }
  return { ok: true };
}

export function normalizeSkillExtraPath(path: string): string {
  return path.trim().replace(/\\/g, "/").replace(/^\/+|\/+$/g, "");
}

export function validateSkillExtraPath(path: string): SkillAssetValidationResult {
  const normalized = normalizeSkillExtraPath(path);
  if (!normalized) {
    return { ok: false, message: "附加文件路径不能为空" };
  }
  if (normalized === "SKILL.md") {
    return { ok: false, message: "SKILL.md 由主编辑区维护" };
  }
  if (normalized.startsWith("skills/")) {
    return { ok: false, message: "附加文件路径应相对 Skill 根目录填写" };
  }
  if (
    normalized.includes(":") ||
    normalized.split("/").some((part) => !part || part === "." || part === "..")
  ) {
    return { ok: false, message: "附加文件路径不能包含空段、冒号、. 或 .." };
  }
  return { ok: true };
}

export function validateSkillAssetDraft(
  draft: SkillAssetDraft,
  existingKeys: string[] = [],
): SkillAssetValidationResult {
  const keyValidation = validateSkillName(draft.key);
  if (!keyValidation.ok) return keyValidation;
  if (!draft.display_name.trim()) {
    return { ok: false, message: "显示名称不能为空" };
  }
  if (!draft.description.trim()) {
    return { ok: false, message: "Skill 描述不能为空" };
  }
  if (draft.description.trim().length > 1024) {
    return { ok: false, message: "Skill 描述不能超过 1024 个字符" };
  }
  if (existingKeys.some((key) => key === draft.key.trim())) {
    return { ok: false, message: `Skill key 已存在：${draft.key.trim()}` };
  }

  const seenPaths = new Set<string>();
  for (const file of draft.files) {
    const pathValidation = validateSkillExtraPath(file.relative_path);
    if (!pathValidation.ok) return pathValidation;
    const normalized = normalizeSkillExtraPath(file.relative_path);
    if (seenPaths.has(normalized)) {
      return { ok: false, message: `附加文件路径重复：${normalized}` };
    }
    seenPaths.add(normalized);
  }

  return { ok: true };
}

export function createEmptySkillAssetDraft(key = ""): SkillAssetDraft {
  return {
    key,
    display_name: "",
    description: "",
    body: "# 使用说明\n",
    disable_model_invocation: false,
    files: [],
  };
}

export async function fetchProjectSkillAssets(
  projectId: string,
  query?: ListSkillAssetQuery,
): Promise<SkillAssetDto[]> {
  const params = new URLSearchParams();
  if (query?.source) params.set("source", query.source);
  const qs = params.toString() ? `?${params}` : "";
  const raw = await api.get<Record<string, unknown>[]>(
    `/projects/${encodeURIComponent(projectId)}/skill-assets${qs}`,
  );
  return raw.map(mapSkillAsset);
}

export async function createSkillAsset(
  projectId: string,
  input: CreateSkillAssetRequest,
): Promise<SkillAssetDto> {
  const raw = await api.post<Record<string, unknown>>(
    `/projects/${encodeURIComponent(projectId)}/skill-assets`,
    input,
  );
  return mapSkillAsset(raw);
}

export async function updateSkillAsset(
  projectId: string,
  assetId: string,
  input: UpdateSkillAssetRequest,
): Promise<SkillAssetDto> {
  const raw = await api.patch<Record<string, unknown>>(
    `/projects/${encodeURIComponent(projectId)}/skill-assets/${encodeURIComponent(assetId)}`,
    input,
  );
  return mapSkillAsset(raw);
}

export async function deleteSkillAsset(
  projectId: string,
  assetId: string,
): Promise<void> {
  await api.delete(
    `/projects/${encodeURIComponent(projectId)}/skill-assets/${encodeURIComponent(assetId)}`,
  );
}

export async function bootstrapSkillAssets(
  projectId: string,
  input: BootstrapSkillAssetRequest = {},
): Promise<SkillAssetDto[]> {
  const raw = await api.post<Record<string, unknown>[]>(
    `/projects/${encodeURIComponent(projectId)}/skill-assets/bootstrap`,
    input,
  );
  return raw.map(mapSkillAsset);
}

export async function resetSkillAssetFromBuiltin(
  projectId: string,
  assetId: string,
): Promise<SkillAssetDto> {
  const raw = await api.post<Record<string, unknown>>(
    `/projects/${encodeURIComponent(projectId)}/skill-assets/${encodeURIComponent(assetId)}/reset-from-builtin`,
    {},
  );
  return mapSkillAsset(raw);
}

export async function uploadSkillAssets(
  projectId: string,
  files: File[],
): Promise<SkillAssetDto[]> {
  const form = new FormData();
  for (const file of files) {
    const relativePath = typeof file.webkitRelativePath === "string" && file.webkitRelativePath
      ? file.webkitRelativePath
      : file.name;
    form.append("files", file, relativePath);
  }
  const response = await authenticatedFetch(
    buildApiPath(`/projects/${encodeURIComponent(projectId)}/skill-assets/upload`),
    {
      method: "POST",
      body: form,
    },
  );
  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(body.error || `HTTP ${response.status}`);
  }
  const raw = await response.json() as Record<string, unknown>[];
  return raw.map(mapSkillAsset);
}
