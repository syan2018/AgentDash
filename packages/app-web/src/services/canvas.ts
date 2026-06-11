import { api } from "../api/client";
import { asRecord, asRecordArray, asStringArray } from "../api/mappers";
import type { ExtensionPackageInstallationResponse } from "../generated/extension-package-contracts";
import type {
  Canvas,
  CanvasDataBinding,
  CanvasFile,
  CanvasImportMap,
  CanvasRuntimeSnapshot,
  CanvasSandboxConfig,
  RuntimeInvocationResult,
} from "../types";

function mapCanvasImportMap(raw: unknown): CanvasImportMap {
  const value = asRecord(raw);
  const imports = asRecord(value?.imports);

  return {
    imports: Object.fromEntries(
      Object.entries(imports ?? {}).map(([key, value]) => [key, String(value ?? "")]),
    ),
  };
}

function mapCanvasSandboxConfig(raw: unknown): CanvasSandboxConfig {
  const value = asRecord(raw);
  return {
    libraries: asStringArray(value?.libraries),
    import_map: mapCanvasImportMap(value?.import_map),
  };
}

function mapCanvasFile(raw: Record<string, unknown>): CanvasFile {
  return {
    path: String(raw.path ?? ""),
    content: String(raw.content ?? ""),
  };
}

function mapCanvasBinding(raw: Record<string, unknown>): CanvasDataBinding {
  return {
    alias: String(raw.alias ?? ""),
    source_uri: String(raw.source_uri ?? ""),
    content_type: String(raw.content_type ?? "application/json"),
  };
}

function mapCanvas(raw: Record<string, unknown>): Canvas {
  return {
    id: String(raw.id ?? ""),
    project_id: String(raw.project_id ?? ""),
    mount_id: String(raw.mount_id ?? ""),
    title: String(raw.title ?? ""),
    description: String(raw.description ?? ""),
    entry_file: String(raw.entry_file ?? ""),
    sandbox_config: mapCanvasSandboxConfig(raw.sandbox_config),
    files: asRecordArray(raw.files).map(mapCanvasFile),
    bindings: asRecordArray(raw.bindings).map(mapCanvasBinding),
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
}

export interface CreateCanvasInput {
  mount_id?: string;
  title: string;
  description?: string;
  entry_file?: string;
  sandbox_config?: CanvasSandboxConfig;
  files?: CanvasFile[];
  bindings?: CanvasDataBinding[];
}

export interface UpdateCanvasInput {
  title?: string;
  description?: string;
  entry_file?: string;
  sandbox_config?: CanvasSandboxConfig;
  files?: CanvasFile[];
  bindings?: CanvasDataBinding[];
}

export async function fetchProjectCanvases(projectId: string): Promise<Canvas[]> {
  const raw = await api.get<Record<string, unknown>[]>(
    `/projects/${encodeURIComponent(projectId)}/canvases`,
  );
  return raw.map(mapCanvas);
}

export async function createCanvas(
  projectId: string,
  input: CreateCanvasInput,
): Promise<Canvas> {
  const raw = await api.post<Record<string, unknown>>(
    `/projects/${encodeURIComponent(projectId)}/canvases`,
    input,
  );
  return mapCanvas(raw);
}

export async function fetchCanvas(canvasId: string): Promise<Canvas> {
  const raw = await api.get<Record<string, unknown>>(`/canvases/${encodeURIComponent(canvasId)}`);
  return mapCanvas(raw);
}

export async function updateCanvas(
  canvasId: string,
  input: UpdateCanvasInput,
): Promise<Canvas> {
  const raw = await api.put<Record<string, unknown>>(
    `/canvases/${encodeURIComponent(canvasId)}`,
    input,
  );
  return mapCanvas(raw);
}

export async function deleteCanvas(canvasId: string): Promise<void> {
  await api.delete(`/canvases/${encodeURIComponent(canvasId)}`);
}

export async function fetchCanvasRuntimeSnapshot(
  canvasId: string,
  sessionId?: string | null,
): Promise<CanvasRuntimeSnapshot> {
  const params = new URLSearchParams();
  if (sessionId) {
    params.set("session_id", sessionId);
  }
  const query = params.toString();
  return api.get<CanvasRuntimeSnapshot>(
    query
      ? `/canvases/${encodeURIComponent(canvasId)}/runtime-snapshot?${query}`
      : `/canvases/${encodeURIComponent(canvasId)}/runtime-snapshot`,
  );
}

export interface CanvasRuntimeInvokeInput {
  session_id: string;
  action_key: string;
  input?: unknown;
}

export interface PromoteCanvasToExtensionInput {
  extension_key?: string;
  display_name?: string;
  package_version?: string;
  asset_version?: string;
  overwrite?: boolean;
}

export async function invokeCanvasRuntimeAction(
  canvasId: string,
  input: CanvasRuntimeInvokeInput,
): Promise<RuntimeInvocationResult> {
  return api.post<RuntimeInvocationResult>(
    `/canvases/${encodeURIComponent(canvasId)}/runtime-invoke`,
    input,
  );
}

export async function promoteCanvasToExtension(
  canvasId: string,
  input: PromoteCanvasToExtensionInput = {},
): Promise<ExtensionPackageInstallationResponse> {
  return api.post<ExtensionPackageInstallationResponse>(
    `/canvases/${encodeURIComponent(canvasId)}/promote-extension`,
    input,
  );
}
