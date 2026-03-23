import { buildApiPath } from "../api/origin";

// ─── Descriptor（能力发现） ─────────────────────────────

export interface SelectorHint {
  trigger: string;
  placeholder: string;
  result_item_type: string;
}

export interface AddressSpaceDescriptor {
  id: string;
  label: string;
  kind: string;
  provider: string;
  supports: string[];
  root?: string | null;
  workspace_id?: string | null;
  selector?: SelectorHint | null;
}

export interface ListAddressSpacesResponse {
  spaces: AddressSpaceDescriptor[];
}

// ─── Entry（条目搜索） ──────────────────────────────────

export interface AddressEntry {
  address: string;
  label: string;
  entry_type: string;
  size?: number | null;
  is_dir?: boolean | null;
}

export interface ListEntriesResponse {
  entries: AddressEntry[];
}

// ─── Mount Entry（mount 级条目） ────────────────────────

export interface MountEntry {
  path: string;
  entry_type: string;
  size?: number | null;
  is_dir: boolean;
}

export interface ListMountEntriesResponse {
  mount_id: string;
  entries: MountEntry[];
}

// ─── Mount 文件读取 ─────────────────────────────────────

export interface ReadMountFileResponse {
  mount_id: string;
  path: string;
  content: string;
  size: number;
}

// ─── Address Space 预览 ─────────────────────────────────

export interface MountSummary {
  id: string;
  provider: string;
  backend_id: string;
  root_ref: string;
  capabilities: string[];
  default_write: boolean;
  display_name: string;
  backend_online?: boolean | null;
  file_count?: number | null;
}

export interface PreviewAddressSpaceResponse {
  mounts: MountSummary[];
  default_mount_id?: string | null;
}

// ─── 查询参数 ────────────────────────────────────────────

export interface AddressSpaceQueryParams {
  workspaceId?: string | null;
}

export interface ListEntriesParams extends AddressSpaceQueryParams {
  query?: string;
  path?: string;
  recursive?: boolean;
}

// ─── API 辅助 ────────────────────────────────────────────

function applyQueryParams(searchParams: URLSearchParams, params?: AddressSpaceQueryParams) {
  if (!params) return;
  if (params.workspaceId) searchParams.set("workspace_id", params.workspaceId);
}

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, init);
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    let message = `HTTP ${res.status}`;
    if (text) {
      try {
        const parsed = JSON.parse(text);
        message = parsed.error ?? parsed.message ?? text;
      } catch {
        message = text;
      }
    }
    throw new Error(message);
  }
  return res.json();
}

// ─── API 函数 ────────────────────────────────────────────

export async function listAddressSpaces(
  params?: AddressSpaceQueryParams,
): Promise<ListAddressSpacesResponse> {
  const searchParams = new URLSearchParams();
  applyQueryParams(searchParams, params);

  const qs = searchParams.toString();
  const url = buildApiPath(`/address-spaces${qs ? `?${qs}` : ""}`);
  return fetchJson(url);
}

export async function listAddressEntries(
  spaceId: string,
  params?: ListEntriesParams,
): Promise<ListEntriesResponse> {
  const searchParams = new URLSearchParams();
  applyQueryParams(searchParams, params);
  if (params?.query) searchParams.set("query", params.query);
  if (params?.path) searchParams.set("path", params.path);
  if (params?.recursive !== undefined) searchParams.set("recursive", String(params.recursive));

  const qs = searchParams.toString();
  const url = buildApiPath(
    `/address-spaces/${encodeURIComponent(spaceId)}/entries${qs ? `?${qs}` : ""}`,
  );
  return fetchJson(url);
}

export async function listMountEntries(params: {
  projectId: string;
  storyId?: string;
  mountId: string;
  path?: string;
  pattern?: string;
  recursive?: boolean;
}): Promise<ListMountEntriesResponse> {
  const searchParams = new URLSearchParams();
  searchParams.set("project_id", params.projectId);
  if (params.storyId) searchParams.set("story_id", params.storyId);
  if (params.path) searchParams.set("path", params.path);
  if (params.pattern) searchParams.set("pattern", params.pattern);
  if (params.recursive !== undefined) searchParams.set("recursive", String(params.recursive));

  const qs = searchParams.toString();
  const url = buildApiPath(
    `/address-spaces/mounts/${encodeURIComponent(params.mountId)}/entries${qs ? `?${qs}` : ""}`,
  );
  return fetchJson(url);
}

export async function readMountFile(params: {
  projectId: string;
  storyId?: string;
  mountId: string;
  path: string;
}): Promise<ReadMountFileResponse> {
  const url = buildApiPath("/address-spaces/read-file");
  return fetchJson(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      project_id: params.projectId,
      story_id: params.storyId,
      mount_id: params.mountId,
      path: params.path,
    }),
  });
}

export async function previewAddressSpace(params: {
  projectId: string;
  storyId?: string;
  target?: "project" | "story" | "task";
}): Promise<PreviewAddressSpaceResponse> {
  const url = buildApiPath("/address-spaces/preview");
  return fetchJson(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      project_id: params.projectId,
      story_id: params.storyId,
      target: params.target ?? "project",
    }),
  });
}
