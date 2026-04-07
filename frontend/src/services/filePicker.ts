import { api } from "../api/client";

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

export async function listFiles(
  workspaceId: string,
  pattern?: string,
): Promise<ListFilesResponse> {
  const params = new URLSearchParams();
  params.set("workspace_id", workspaceId);
  if (pattern) params.set("pattern", pattern);

  return api.get<ListFilesResponse>(`/file-picker?${params}`);
}

export async function batchReadFiles(
  workspaceId: string,
  paths: string[],
): Promise<BatchReadFilesResponse> {
  return api.post<BatchReadFilesResponse>("/file-picker/batch-read", {
    workspace_id: workspaceId,
    paths,
  });
}
