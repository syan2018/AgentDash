import { api, authenticatedFetch } from "../api/client";
import { buildApiPath } from "../api/origin";
import type {
  ListEntriesResponse,
  ListVfssResponse,
  ResolvedVfsSurface,
  ResolvedVfsSurfaceSource,
  SurfaceApplyPatchResponse,
  SurfaceCreateFileResponse,
  SurfaceDeleteFileResponse,
  SurfaceEntriesResponse,
  SurfaceMountEntry,
  SurfaceReadFileResponse,
  SurfaceRenameFileResponse,
  SurfaceStatFileResponse,
  SurfaceUploadBinaryFileResponse,
  SurfaceWriteFileResponse,
  VfsDescriptor,
  VfsEntry,
} from "../generated/vfs-contracts";
import { vfsRoutes } from "./vfsRoutes";

export type {
  ListEntriesResponse,
  ListVfssResponse,
  SurfaceApplyPatchResponse as ApplySurfacePatchResponse,
  SurfaceCreateFileResponse as CreateSurfaceFileResponse,
  SurfaceDeleteFileResponse as DeleteSurfaceFileResponse,
  SurfaceEntriesResponse as ListSurfaceMountEntriesResponse,
  SurfaceMountEntry,
  SurfaceReadFileResponse as ReadSurfaceFileResponse,
  SurfaceRenameFileResponse as RenameSurfaceFileResponse,
  SurfaceStatFileResponse as StatSurfaceFileResponse,
  SurfaceUploadBinaryFileResponse as UploadSurfaceFileBlobResponse,
  SurfaceWriteFileResponse as WriteSurfaceFileResponse,
  VfsDescriptor,
  VfsEntry,
};

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

export async function listVfss(
  params?: VfsQueryParams,
): Promise<ListVfssResponse> {
  const sp = new URLSearchParams();
  applyQueryParams(sp, params);
  return api.get<ListVfssResponse>(vfsRoutes.spaces(sp));
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

  return api.get<ListEntriesResponse>(vfsRoutes.entries(spaceId, sp));
}

export async function resolveVfsSurface(
  source: ResolvedVfsSurfaceSource,
): Promise<ResolvedVfsSurface> {
  return api.post<ResolvedVfsSurface>(vfsRoutes.surfaces.resolve, { source });
}

export async function getVfsSurface(surfaceRef: string): Promise<ResolvedVfsSurface> {
  return api.get<ResolvedVfsSurface>(vfsRoutes.surfaces.byRef(surfaceRef));
}

export async function listSurfaceMountEntries(params: {
  surfaceRef: string;
  mountId: string;
  path?: string;
  pattern?: string;
  recursive?: boolean;
}): Promise<SurfaceEntriesResponse> {
  const sp = new URLSearchParams();
  if (params.path) sp.set("path", params.path);
  if (params.pattern) sp.set("pattern", params.pattern);
  if (params.recursive !== undefined) sp.set("recursive", String(params.recursive));

  return api.get<SurfaceEntriesResponse>(
    vfsRoutes.surfaces.entries(params.surfaceRef, params.mountId, sp),
  );
}

export async function readSurfaceFile(params: {
  surfaceRef: string;
  mountId: string;
  path: string;
}): Promise<SurfaceReadFileResponse> {
  return api.post<SurfaceReadFileResponse>(vfsRoutes.surfaces.readFile, {
    surface_ref: params.surfaceRef,
    mount_id: params.mountId,
    path: params.path,
  });
}

export async function readSurfaceFileBlob(params: {
  surfaceRef: string;
  mountId: string;
  path: string;
}): Promise<Blob> {
  const response = await authenticatedFetch(buildApiPath(vfsRoutes.surfaces.readFileBlob), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      surface_ref: params.surfaceRef,
      mount_id: params.mountId,
      path: params.path,
    }),
  });
  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(String(body.error || `HTTP ${response.status}`));
  }
  return response.blob();
}

export async function writeSurfaceFile(params: {
  surfaceRef: string;
  mountId: string;
  path: string;
  content: string;
}): Promise<SurfaceWriteFileResponse> {
  return api.post<SurfaceWriteFileResponse>(vfsRoutes.surfaces.writeFile, {
    surface_ref: params.surfaceRef,
    mount_id: params.mountId,
    path: params.path,
    content: params.content,
  });
}

export async function createSurfaceFile(params: {
  surfaceRef: string;
  mountId: string;
  path: string;
  content: string;
}): Promise<SurfaceCreateFileResponse> {
  return api.post<SurfaceCreateFileResponse>(vfsRoutes.surfaces.createFile, {
    surface_ref: params.surfaceRef,
    mount_id: params.mountId,
    path: params.path,
    content: params.content,
  });
}

export async function uploadSurfaceFileBlob(params: {
  surfaceRef: string;
  mountId: string;
  path?: string;
  file: File;
}): Promise<SurfaceUploadBinaryFileResponse> {
  const form = new FormData();
  form.append("surface_ref", params.surfaceRef);
  form.append("mount_id", params.mountId);
  if (params.path) form.append("path", params.path);
  form.append("file", params.file, params.file.name);

  const response = await authenticatedFetch(buildApiPath(vfsRoutes.surfaces.uploadFileBlob), {
    method: "POST",
    body: form,
  });
  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(String(body.error || `HTTP ${response.status}`));
  }
  return response.json();
}

export async function deleteSurfaceFile(params: {
  surfaceRef: string;
  mountId: string;
  path: string;
}): Promise<SurfaceDeleteFileResponse> {
  return api.post<SurfaceDeleteFileResponse>(vfsRoutes.surfaces.deleteFile, {
    surface_ref: params.surfaceRef,
    mount_id: params.mountId,
    path: params.path,
  });
}

export async function renameSurfaceFile(params: {
  surfaceRef: string;
  mountId: string;
  fromPath: string;
  toPath: string;
}): Promise<SurfaceRenameFileResponse> {
  return api.post<SurfaceRenameFileResponse>(vfsRoutes.surfaces.renameFile, {
    surface_ref: params.surfaceRef,
    mount_id: params.mountId,
    from_path: params.fromPath,
    to_path: params.toPath,
  });
}

export async function statSurfaceFile(params: {
  surfaceRef: string;
  mountId: string;
  path: string;
}): Promise<SurfaceStatFileResponse> {
  return api.post<SurfaceStatFileResponse>(vfsRoutes.surfaces.statFile, {
    surface_ref: params.surfaceRef,
    mount_id: params.mountId,
    path: params.path,
  });
}

export async function applySurfacePatch(params: {
  surfaceRef: string;
  mountId: string;
  patch: string;
}): Promise<SurfaceApplyPatchResponse> {
  return api.post<SurfaceApplyPatchResponse>(vfsRoutes.surfaces.applyPatch, {
    surface_ref: params.surfaceRef,
    mount_id: params.mountId,
    patch: params.patch,
  });
}
