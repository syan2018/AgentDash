import { api } from "../api/client";
import type { ResolvedAddressSpaceSurface, ResolvedAddressSpaceSurfaceSource } from "../types/context";

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

export interface AddressSpaceQueryParams {
  workspaceId?: string | null;
}

export interface ListEntriesParams extends AddressSpaceQueryParams {
  query?: string;
  path?: string;
  recursive?: boolean;
}

function applyQueryParams(searchParams: URLSearchParams, params?: AddressSpaceQueryParams) {
  if (!params) return;
  if (params.workspaceId) searchParams.set("workspace_id", params.workspaceId);
}

function buildQs(searchParams: URLSearchParams): string {
  const qs = searchParams.toString();
  return qs ? `?${qs}` : "";
}

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

export async function resolveAddressSpaceSurface(
  source: ResolvedAddressSpaceSurfaceSource,
): Promise<ResolvedAddressSpaceSurface> {
  return api.post<ResolvedAddressSpaceSurface>("/address-space-surfaces/resolve", { source });
}

export async function getAddressSpaceSurface(surfaceRef: string): Promise<ResolvedAddressSpaceSurface> {
  return api.get<ResolvedAddressSpaceSurface>(`/address-space-surfaces/${encodeURIComponent(surfaceRef)}`);
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
    `/address-space-surfaces/${encodeURIComponent(params.surfaceRef)}/mounts/${encodeURIComponent(params.mountId)}/entries${buildQs(sp)}`,
  );
}

export async function readSurfaceFile(params: {
  surfaceRef: string;
  mountId: string;
  path: string;
}): Promise<ReadSurfaceFileResponse> {
  return api.post<ReadSurfaceFileResponse>("/address-space-surfaces/read-file", {
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
  return api.post<WriteSurfaceFileResponse>("/address-space-surfaces/write-file", {
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
  return api.post<ApplySurfacePatchResponse>("/address-space-surfaces/apply-patch", {
    surface_ref: params.surfaceRef,
    mount_id: params.mountId,
    patch: params.patch,
  });
}
