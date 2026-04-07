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

export interface CanvasRuntimeSnapshot {
  canvas_id: string;
  session_id?: string | null;
  entry: string;
  files: CanvasRuntimeFile[];
  bindings: CanvasRuntimeBinding[];
  import_map: CanvasImportMap;
  libraries: string[];
}