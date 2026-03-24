import { buildApiPath } from "../api/origin";

export interface FileEntry {
  relPath: string;
  size: number;
  isText: boolean;
}

export interface ListFilesResponse {
  files: FileEntry[];
  root: string;
}

export interface ReadFileResponse {
  relPath: string;
  uri: string;
  mimeType: string;
  content: string;
  size: number;
}

export interface ReadFileResult {
  relPath: string;
  uri: string;
  mimeType: string;
  content: string | null;
  size: number;
  error: string | null;
}

export interface BatchReadFilesResponse {
  files: ReadFileResult[];
  totalSize: number;
}

export async function listWorkspaceFiles(
  workspaceId: string,
  pattern?: string,
): Promise<ListFilesResponse> {
  const params = new URLSearchParams();
  params.set("workspace_id", workspaceId);
  if (pattern) params.set("pattern", pattern);

  const url = buildApiPath(`/workspace-files${params.toString() ? `?${params}` : ""}`);
  const res = await fetch(url);

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `listWorkspaceFiles failed: HTTP ${res.status}`);
  }

  return res.json();
}

export async function batchReadWorkspaceFiles(
  workspaceId: string,
  paths: string[],
): Promise<BatchReadFilesResponse> {
  const url = buildApiPath("/workspace-files/batch-read");
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ workspace_id: workspaceId, paths }),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `batchReadWorkspaceFiles failed: HTTP ${res.status}`);
  }

  return res.json();
}
