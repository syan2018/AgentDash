import { api } from "../api/client";

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

// ─── Mount 文件写入 ─────────────────────────────────────

export interface WriteMountFileResponse {
  mount_id: string;
  path: string;
  size: number;
  persisted: boolean;
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

function buildQs(searchParams: URLSearchParams): string {
  const qs = searchParams.toString();
  return qs ? `?${qs}` : "";
}

// ─── API 函数 ────────────────────────────────────────────

export async function listAddressSpaces(
  params?: AddressSpaceQueryParams,
): Promise<ListAddressSpacesResponse> {
  const sp = new URLSearchParams();
  applyQueryParams(sp, params);
  return api.get<ListAddressSpacesResponse>(`/address-spaces${buildQs(sp)}`);
}

export async function listAddressEntries(
  spaceId: string,
  params?: ListEntriesParams,
): Promise<ListEntriesResponse> {
  const sp = new URLSearchParams();
  applyQueryParams(sp, params);
  if (params?.query) sp.set("query", params.query);
  if (params?.path) sp.set("path", params.path);
  if (params?.recursive !== undefined) sp.set("recursive", String(params.recursive));

  return api.get<ListEntriesResponse>(
    `/address-spaces/${encodeURIComponent(spaceId)}/entries${buildQs(sp)}`,
  );
}

export async function listMountEntries(params: {
  projectId: string;
  storyId?: string;
  ownerType?: string;
  ownerId?: string;
  agentId?: string;
  mountId: string;
  path?: string;
  pattern?: string;
  recursive?: boolean;
}): Promise<ListMountEntriesResponse> {
  const sp = new URLSearchParams();
  sp.set("project_id", params.projectId);
  if (params.storyId) sp.set("story_id", params.storyId);
  if (params.ownerType) sp.set("owner_type", params.ownerType);
  if (params.ownerId) sp.set("owner_id", params.ownerId);
  if (params.agentId) sp.set("agent_id", params.agentId);
  if (params.path) sp.set("path", params.path);
  if (params.pattern) sp.set("pattern", params.pattern);
  if (params.recursive !== undefined) sp.set("recursive", String(params.recursive));

  return api.get<ListMountEntriesResponse>(
    `/address-spaces/mounts/${encodeURIComponent(params.mountId)}/entries${buildQs(sp)}`,
  );
}

export async function readMountFile(params: {
  projectId: string;
  storyId?: string;
  ownerType?: string;
  ownerId?: string;
  agentId?: string;
  mountId: string;
  path: string;
}): Promise<ReadMountFileResponse> {
  return api.post<ReadMountFileResponse>("/address-spaces/read-file", {
    project_id: params.projectId,
    story_id: params.storyId,
    owner_type: params.ownerType,
    owner_id: params.ownerId,
    agent_id: params.agentId,
    mount_id: params.mountId,
    path: params.path,
  });
}

export async function previewAddressSpace(params: {
  projectId: string;
  storyId?: string;
  ownerType?: string;
  ownerId?: string;
  agentId?: string;
  target?: "project" | "story" | "task";
}): Promise<PreviewAddressSpaceResponse> {
  return api.post<PreviewAddressSpaceResponse>("/address-spaces/preview", {
    project_id: params.projectId,
    story_id: params.storyId,
    owner_type: params.ownerType,
    owner_id: params.ownerId,
    agent_id: params.agentId,
    target: params.target ?? "project",
  });
}

export async function writeMountFile(params: {
  projectId: string;
  storyId?: string;
  ownerType?: string;
  ownerId?: string;
  agentId?: string;
  mountId: string;
  path: string;
  content: string;
}): Promise<WriteMountFileResponse> {
  return api.post<WriteMountFileResponse>("/address-spaces/write-file", {
    project_id: params.projectId,
    story_id: params.storyId,
    owner_type: params.ownerType,
    owner_id: params.ownerId,
    agent_id: params.agentId,
    mount_id: params.mountId,
    path: params.path,
    content: params.content,
  });
}
