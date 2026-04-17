import { api } from "../api/client";
import type { ResolvedVfsSurface, ResolvedVfsSurfaceSource } from "../types/context";

// ─── Descriptor（能力发现） ─────────────────────────────

export interface SelectorHint {
  trigger: string;
  placeholder: string;
  result_item_type: string;
}

export interface VfsDescriptor {
  id: string;
  label: string;
  kind: string;
  provider: string;
  supports: string[];
  root?: string | null;
  workspace_id?: string | null;
  selector?: SelectorHint | null;
}

export interface ListVfssResponse {
  spaces: VfsDescriptor[];
}

// ─── Entry（条目搜索） ──────────────────────────────────

export interface VfsEntry {
  address: string;
  label: string;
  entry_type: string;
  size?: number | null;
  is_dir?: boolean | null;
}

export interface ListEntriesResponse {
  entries: VfsEntry[];
}

// ─── Surface Mount Entry ───────────────────────────────

export interface SurfaceMountEntry {
  path: string;
  entry_type: string;
  size?: number | null;
  is_dir: boolean;
}

export interface ListSurfaceMountEntriesResponse {
  surface_ref: string;
  mount_id: string;
  entries: SurfaceMountEntry[];
}

export interface ReadSurfaceFileResponse {
  surface_ref: string;
  mount_id: string;
  path: string;
  content: string;
  size: number;
}

export interface WriteSurfaceFileResponse {
  surface_ref: string;
  mount_id: string;
  path: string;
  size: number;
  persisted: boolean;
}

export interface ApplySurfacePatchResponse {
  surface_ref: string;
  mount_id: string;
  added: string[];
  modified: string[];
  deleted: string[];
}

export interface VfsQueryParams {
  workspaceId?: string | null;
}

export interface ListEntriesParams extends VfsQueryParams {
  query?: string;
  path?: string;
  recursive?: boolean;
}

function applyQueryParams(searchParams: URLSearchParams, params?: VfsQueryParams) {
  if (!params) return;
  if (params.workspaceId) searchParams.set("workspace_id", params.workspaceId);
}

function buildQs(searchParams: URLSearchParams): string {
  const qs = searchParams.toString();
  return qs ? `?${qs}` : "";
}

export async function listVfss(
  params?: VfsQueryParams,
): Promise<ListVfssResponse> {
  const sp = new URLSearchParams();
  applyQueryParams(sp, params);
  return api.get<ListVfssResponse>(`/vfs${buildQs(sp)}`);
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
    `/vfs/${encodeURIComponent(spaceId)}/entries${buildQs(sp)}`,
  );
}

export async function resolveVfsSurface(
  source: ResolvedVfsSurfaceSource,
): Promise<ResolvedVfsSurface> {
  return api.post<ResolvedVfsSurface>("/vfs-surfaces/resolve", { source });
}

export async function getVfsSurface(surfaceRef: string): Promise<ResolvedVfsSurface> {
  return api.get<ResolvedVfsSurface>(`/vfs-surfaces/${encodeURIComponent(surfaceRef)}`);
}

export async function listSurfaceMountEntries(params: {
  surfaceRef: string;
  mountId: string;
  path?: string;
  pattern?: string;
  recursive?: boolean;
}): Promise<ListSurfaceMountEntriesResponse> {
  const sp = new URLSearchParams();
  if (params.path) sp.set("path", params.path);
  if (params.pattern) sp.set("pattern", params.pattern);
  if (params.recursive !== undefined) sp.set("recursive", String(params.recursive));

  return api.get<ListSurfaceMountEntriesResponse>(
    `/vfs-surfaces/${encodeURIComponent(params.surfaceRef)}/mounts/${encodeURIComponent(params.mountId)}/entries${buildQs(sp)}`,
  );
}

export async function readSurfaceFile(params: {
  surfaceRef: string;
  mountId: string;
  path: string;
}): Promise<ReadSurfaceFileResponse> {
  return api.post<ReadSurfaceFileResponse>("/vfs-surfaces/read-file", {
    surface_ref: params.surfaceRef,
    mount_id: params.mountId,
    path: params.path,
  });
}

export async function writeSurfaceFile(params: {
  surfaceRef: string;
  mountId: string;
  path: string;
  content: string;
}): Promise<WriteSurfaceFileResponse> {
  return api.post<WriteSurfaceFileResponse>("/vfs-surfaces/write-file", {
    surface_ref: params.surfaceRef,
    mount_id: params.mountId,
    path: params.path,
    content: params.content,
  });
}

export async function applySurfacePatch(params: {
  surfaceRef: string;
  mountId: string;
  patch: string;
}): Promise<ApplySurfacePatchResponse> {
  return api.post<ApplySurfacePatchResponse>("/vfs-surfaces/apply-patch", {
    surface_ref: params.surfaceRef,
    mount_id: params.mountId,
    patch: params.patch,
  });
}
