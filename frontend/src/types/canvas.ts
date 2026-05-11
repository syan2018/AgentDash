// ─── Canvas ──────────────────────────────────────────

export interface CanvasImportMap {
  imports: Record<string, string>;
}

export interface CanvasSandboxConfig {
  libraries: string[];
  import_map: CanvasImportMap;
}

export interface CanvasFile {
  path: string;
  content: string;
}

export interface CanvasDataBinding {
  alias: string;
  source_uri: string;
  content_type: string;
}

export interface Canvas {
  id: string;
  project_id: string;
  mount_id: string;
  title: string;
  description: string;
  entry_file: string;
  sandbox_config: CanvasSandboxConfig;
  files: CanvasFile[];
  bindings: CanvasDataBinding[];
  created_at: string;
  updated_at: string;
}

export interface CanvasRuntimeFile {
  path: string;
  content: string;
  file_type: string;
}

export interface CanvasRuntimeBinding {
  alias: string;
  source_uri: string;
  data_path: string;
  content_type: string;
  resolved: boolean;
}

export type RuntimeActionKind = "session_runtime" | "setup";

export interface RuntimePolicy {
  required_capabilities: string[];
  timeout_ms?: number | null;
  allow_background: boolean;
}

export interface RuntimeActionDescriptor {
  action_key: string;
  kind: RuntimeActionKind;
  description?: string | null;
  input_schema?: unknown;
  output_schema?: unknown;
  default_policy: RuntimePolicy;
}

export type RuntimeContext =
  | {
      type: "session";
      session_id: string;
      project_id?: string | null;
      workspace_id?: string | null;
    }
  | {
      type: "setup";
      project_id?: string | null;
      workspace_id?: string | null;
      backend_id?: string | null;
      root_ref?: string | null;
    };

export interface RuntimeSurface {
  context: RuntimeContext;
  actions: RuntimeActionDescriptor[];
}

export interface CanvasRuntimeBridgeSnapshot {
  enabled: boolean;
  surface?: RuntimeSurface | null;
  disabled_reason?: string | null;
}

export interface CanvasRuntimeSnapshot {
  canvas_id: string;
  session_id?: string | null;
  entry: string;
  files: CanvasRuntimeFile[];
  bindings: CanvasRuntimeBinding[];
  import_map: CanvasImportMap;
  libraries: string[];
  runtime_bridge: CanvasRuntimeBridgeSnapshot;
}

export interface RuntimeTrace {
  trace_id: string;
  invocation_id: string;
  parent_trace_id?: string | null;
  created_at: string;
}

export interface RuntimeInvocationOutput {
  output: unknown;
  metadata: Record<string, unknown>;
}

export interface RuntimeInvocationResult {
  action_key: string;
  trace: RuntimeTrace;
  output: RuntimeInvocationOutput;
}
