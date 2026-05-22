import { api } from "../api/client";
import { asRecord, asRecordArray, asStringArray } from "../api/mappers";
import type {
  Canvas,
  CanvasDataBinding,
  CanvasFile,
  CanvasImportMap,
  CanvasRuntimeBinding,
  CanvasRuntimeBridgeSnapshot,
  CanvasRuntimeFile,
  CanvasRuntimeSnapshot,
  CanvasSandboxConfig,
  RuntimeActionDescriptor,
  RuntimeContext,
  RuntimeInvocationResult,
  RuntimePolicy,
  RuntimeSurface,
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

function mapRuntimePolicy(raw: unknown): RuntimePolicy {
  const value = asRecord(raw);
  return {
    required_capabilities: asStringArray(value?.required_capabilities),
    timeout_ms: typeof value?.timeout_ms === "number" ? value.timeout_ms : null,
    allow_background: Boolean(value?.allow_background),
  };
}

function mapRuntimeActionDescriptor(raw: Record<string, unknown>): RuntimeActionDescriptor {
  return {
    action_key: String(raw.action_key ?? ""),
    kind: raw.kind === "setup" ? "setup" : "session_runtime",
    description: raw.description != null ? String(raw.description) : null,
    input_schema: raw.input_schema,
    output_schema: raw.output_schema,
    default_policy: mapRuntimePolicy(raw.default_policy),
  };
}

function mapRuntimeContext(raw: unknown): RuntimeContext {
  const value = asRecord(raw);
  if (value?.type === "setup") {
    return {
      type: "setup",
      project_id: value.project_id != null ? String(value.project_id) : null,
      workspace_id: value.workspace_id != null ? String(value.workspace_id) : null,
      backend_id: value.backend_id != null ? String(value.backend_id) : null,
      root_ref: value.root_ref != null ? String(value.root_ref) : null,
    };
  }

  return {
    type: "session",
    session_id: String(value?.session_id ?? ""),
    project_id: value?.project_id != null ? String(value.project_id) : null,
    workspace_id: value?.workspace_id != null ? String(value.workspace_id) : null,
  };
}

function mapRuntimeSurface(raw: unknown): RuntimeSurface | null {
  const value = asRecord(raw);
  if (!value) {
    return null;
  }

  return {
    context: mapRuntimeContext(value.context),
    actions: asRecordArray(value.actions).map(mapRuntimeActionDescriptor),
  };
}

function mapCanvasRuntimeBridge(raw: unknown): CanvasRuntimeBridgeSnapshot {
  const value = asRecord(raw);
  return {
    enabled: Boolean(value?.enabled),
    surface: mapRuntimeSurface(value?.surface),
    disabled_reason: value?.disabled_reason != null ? String(value.disabled_reason) : null,
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
    resource_surface_ref: raw.resource_surface_ref != null ? String(raw.resource_surface_ref) : null,
    entry: String(raw.entry ?? ""),
    files: asRecordArray(raw.files).map(mapCanvasRuntimeFile),
    bindings: asRecordArray(raw.bindings).map(mapCanvasRuntimeBinding),
    import_map: mapCanvasImportMap(raw.import_map),
    libraries: asStringArray(raw.libraries),
    runtime_bridge: mapCanvasRuntimeBridge(raw.runtime_bridge),
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

export interface CanvasRuntimeInvokeInput {
  session_id: string;
  action_key: string;
  input?: unknown;
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
