import { api } from "../api/client";
import { asRecord, asRecordArray, asStringArray } from "../api/mappers";
import type {
  Canvas,
  CanvasDataBinding,
  CanvasFile,
  CanvasImportMap,
  CanvasRuntimeBinding,
  CanvasRuntimeFile,
  CanvasRuntimeSnapshot,
  CanvasSandboxConfig,
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

function mapCanvasRuntimeFile(raw: Record<string, unknown>): CanvasRuntimeFile {
  return {
    path: String(raw.path ?? ""),
    content: String(raw.content ?? ""),
    file_type: String(raw.file_type ?? "code"),
  };
}

function mapCanvasRuntimeBinding(raw: Record<string, unknown>): CanvasRuntimeBinding {
  return {
    alias: String(raw.alias ?? ""),
    source_uri: String(raw.source_uri ?? ""),
    data_path: String(raw.data_path ?? ""),
    content_type: String(raw.content_type ?? "application/json"),
    resolved: Boolean(raw.resolved),
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

function mapCanvasRuntimeSnapshot(raw: Record<string, unknown>): CanvasRuntimeSnapshot {
  return {
    canvas_id: String(raw.canvas_id ?? ""),
    session_id: raw.session_id != null ? String(raw.session_id) : null,
    entry: String(raw.entry ?? ""),
    files: asRecordArray(raw.files).map(mapCanvasRuntimeFile),
    bindings: asRecordArray(raw.bindings).map(mapCanvasRuntimeBinding),
    import_map: mapCanvasImportMap(raw.import_map),
    libraries: asStringArray(raw.libraries),
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
  const raw = await api.get<Record<string, unknown>>(
    query
      ? `/canvases/${encodeURIComponent(canvasId)}/runtime-snapshot?${query}`
      : `/canvases/${encodeURIComponent(canvasId)}/runtime-snapshot`,
  );
  return mapCanvasRuntimeSnapshot(raw);
}
