import { api, authenticatedFetch } from "../api/client";
import { buildApiPath } from "../api/origin";
import { asRecord } from "../api/mappers";
import type {
  BootstrapSkillAssetRequest,
  CreateSkillAssetRequest,
  ImportRemoteSkillAssetRequest,
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

export interface ParsedSkillMarkdown {
  name: string | null;
  description: string | null;
  disable_model_invocation: boolean;
  frontmatter: string | null;
  body: string;
}

export interface SkillMarkdownFrontmatterPatch {
  name?: string;
  description?: string;
  disable_model_invocation?: boolean;
}

interface FrontmatterEntry {
  key: string | null;
  lines: string[];
  value: string | null;
}

function normalizeSource(value: unknown): SkillAssetSource {
  if (value === "builtin_seed" || value === "github") return value;
  return "user";
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
    remote_source: mapRemoteSource(raw.remote_source),
    disable_model_invocation: Boolean(raw.disable_model_invocation),
    files,
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
}

function mapRemoteSource(raw: unknown): SkillAssetDto["remote_source"] {
  if (raw === null || raw === undefined) return null;
  const value = asRecord(raw);
  if (!value) return null;
  return {
    source_type: String(value.source_type ?? ""),
    url: String(value.url ?? ""),
    imported_at: String(value.imported_at ?? ""),
    digest: String(value.digest ?? ""),
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

function countLeadingSpaces(value: string): number {
  return value.match(/^ */)?.[0].length ?? 0;
}

function isBlockScalarHeader(value: string): boolean {
  return /^[>|][+-]?\d*$/.test(value.trim());
}

function parseBlockScalar(header: string, rawLines: string[]): string {
  const indicator = header.trim()[0];
  const chomp = header.trim().includes("-") ? "strip" : "clip";
  const nonEmptyIndents = rawLines
    .filter((line) => line.trim().length > 0)
    .map(countLeadingSpaces);
  const indent = nonEmptyIndents.length > 0 ? Math.min(...nonEmptyIndents) : 0;
  const lines = rawLines.map((line) => line.slice(Math.min(indent, line.length)));
  const parsed = indicator === "|" ? lines.join("\n") : foldYamlBlockLines(lines);
  return chomp === "strip" ? parsed.replace(/\n+$/g, "") : `${parsed.replace(/\n+$/g, "")}\n`;
}

function foldYamlBlockLines(lines: string[]): string {
  let result = "";
  let previousBlank = true;
  for (const line of lines) {
    if (line.trim().length === 0) {
      result = result.replace(/[ ]+$/g, "");
      result += "\n";
      previousBlank = true;
      continue;
    }
    if (result && !previousBlank && !result.endsWith("\n")) {
      result += " ";
    }
    result += line.trim();
    previousBlank = false;
  }
  return result;
}

function parseFrontmatterEntries(frontmatter: string): FrontmatterEntry[] {
  const lines = frontmatter.split(/\r?\n/);
  const entries: FrontmatterEntry[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];
    const match = /^(\s*)([A-Za-z0-9_-]+):\s*(.*)$/.exec(line);
    if (!match) {
      entries.push({ key: null, lines: [line], value: null });
      index += 1;
      continue;
    }

    const [, indent, key, rawValue] = match;
    if (!isBlockScalarHeader(rawValue)) {
      entries.push({
        key,
        lines: [line],
        value: parseFrontmatterValue(rawValue),
      });
      index += 1;
      continue;
    }

    const baseIndent = indent.length;
    let endIndex = index + 1;
    while (endIndex < lines.length) {
      const nextLine = lines[endIndex];
      if (nextLine.trim().length === 0) {
        endIndex += 1;
        continue;
      }
      const nextIndent = countLeadingSpaces(nextLine);
      const nextLooksLikeTopLevelKey = /^[A-Za-z0-9_-]+:/.test(nextLine.trimStart());
      if (nextIndent <= baseIndent && nextLooksLikeTopLevelKey) break;
      endIndex += 1;
    }

    const blockLines = lines.slice(index + 1, endIndex);
    entries.push({
      key,
      lines: lines.slice(index, endIndex),
      value: parseBlockScalar(rawValue, blockLines),
    });
    index = endIndex;
  }

  return entries;
}

function readFrontmatterParts(content: string): {
  leading: string;
  frontmatter: string;
  body: string;
} | null {
  const leadingLength = content.length - content.trimStart().length;
  const leading = content.slice(0, leadingLength);
  const normalized = content.slice(leadingLength);
  const match = /^(?:---\r?\n)([\s\S]*?)(?:\r?\n---)(?:\r?\n)?/.exec(normalized);
  if (!match) return null;
  return {
    leading,
    frontmatter: match[1],
    body: normalized.slice(match[0].length),
  };
}

export function parseSkillMarkdown(content: string): ParsedSkillMarkdown {
  const parts = readFrontmatterParts(content);
  if (!parts) {
    return {
      name: null,
      description: null,
      disable_model_invocation: false,
      frontmatter: null,
      body: content,
    };
  }

  let name: string | null = null;
  let description: string | null = null;
  let disableModelInvocation = false;

  for (const entry of parseFrontmatterEntries(parts.frontmatter)) {
    if (entry.key === "name") {
      name = entry.value;
    } else if (entry.key === "description") {
      description = entry.value;
    } else if (entry.key === "disable-model-invocation") {
      disableModelInvocation = entry.value === "true";
    }
  }

  return {
    name,
    description,
    disable_model_invocation: disableModelInvocation,
    frontmatter: parts.frontmatter,
    body: parts.body,
  };
}

function quotedYamlString(value: string): string {
  return JSON.stringify(value);
}

function yamlScalarField(key: string, value: string): string {
  const normalized = value.replace(/\r\n/g, "\n").replace(/\r/g, "\n").trim();
  if (!normalized.includes("\n")) {
    return `${key}: ${quotedYamlString(normalized)}`;
  }
  return [`${key}: |-`, ...normalized.split("\n").map((line) => `  ${line}`)].join("\n");
}

export function buildSkillMarkdown(asset: SkillAssetDraft): string {
  const frontmatter = buildSkillYamlFrontmatter(asset);
  const body = asset.body.trimEnd();
  return body ? `${frontmatter}\n${body}\n` : `${frontmatter}\n`;
}

export function buildSkillYamlFrontmatter(asset: SkillAssetDraft): string {
  return [
    "---",
    `name: ${asset.key.trim()}`,
    yamlScalarField("description", asset.description),
    ...(asset.disable_model_invocation ? ["disable-model-invocation: true"] : []),
    "---",
  ].join("\n");
}

export function updateSkillMarkdownFrontmatter(
  content: string,
  patch: SkillMarkdownFrontmatterPatch,
): string {
  const parts = readFrontmatterParts(content);
  const parsed = parseSkillMarkdown(content);
  const patches = {
    name: Object.prototype.hasOwnProperty.call(patch, "name"),
    description: Object.prototype.hasOwnProperty.call(patch, "description"),
    disable_model_invocation: Object.prototype.hasOwnProperty.call(
      patch,
      "disable_model_invocation",
    ),
  };
  const nextName = patch.name ?? parsed.name;
  const nextDescription = patch.description ?? parsed.description;
  const nextDisableModelInvocation =
    patch.disable_model_invocation ?? parsed.disable_model_invocation;

  if (!parts) {
    const draft = createEmptySkillAssetDraft(nextName ?? "");
    draft.description = nextDescription ?? "";
    draft.disable_model_invocation = nextDisableModelInvocation;
    return `${buildSkillYamlFrontmatter(draft)}\n${content}`;
  }

  const entries = parseFrontmatterEntries(parts.frontmatter);
  const touched = {
    name: false,
    description: false,
    disable_model_invocation: false,
  };
  const nextLines: string[] = [];

  for (const entry of entries) {
    if (entry.key === "name") {
      nextLines.push(
        patches.name && nextName != null ? `name: ${nextName.trim()}` : entry.lines.join("\n"),
      );
      touched.name = true;
      continue;
    }
    if (entry.key === "description") {
      nextLines.push(
        patches.description && nextDescription != null
          ? yamlScalarField("description", nextDescription)
          : entry.lines.join("\n"),
      );
      touched.description = true;
      continue;
    }
    if (entry.key === "disable-model-invocation") {
      if (patches.disable_model_invocation) {
        if (nextDisableModelInvocation) nextLines.push("disable-model-invocation: true");
      } else {
        nextLines.push(entry.lines.join("\n"));
      }
      touched.disable_model_invocation = true;
      continue;
    }
    nextLines.push(entry.lines.join("\n"));
  }

  if (!touched.name && patches.name && nextName != null) {
    nextLines.unshift(`name: ${nextName.trim()}`);
  }
  if (!touched.description && patches.description && nextDescription != null) {
    const insertAt = nextLines.findIndex((line) => /^name:/.test(line.trim()));
    nextLines.splice(
      insertAt >= 0 ? insertAt + 1 : nextLines.length,
      0,
      yamlScalarField("description", nextDescription),
    );
  }
  if (
    !touched.disable_model_invocation &&
    patches.disable_model_invocation &&
    nextDisableModelInvocation
  ) {
    nextLines.push("disable-model-invocation: true");
  }

  return `${parts.leading}---\n${nextLines.join("\n")}\n---\n${parts.body}`;
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

export async function importRemoteSkillAsset(
  projectId: string,
  input: ImportRemoteSkillAssetRequest,
): Promise<SkillAssetDto> {
  const raw = await api.post<Record<string, unknown>>(
    `/projects/${encodeURIComponent(projectId)}/skill-assets/import`,
    input,
  );
  return mapSkillAsset(raw);
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
