import { api } from "../api/client";

export interface BrowseDirectoryEntry {
  name: string;
  path: string;
  is_dir: boolean;
}

export interface BrowseDirectoryResult {
  current_path: string;
  entries: BrowseDirectoryEntry[];
}

export async function browseDirectory(
  backendId: string,
  path?: string,
): Promise<BrowseDirectoryResult> {
  return api.post<BrowseDirectoryResult>(`/backends/${backendId}/browse`, {
    path: path ?? null,
  });
}
