import { api } from "../api/client";
import type {
  DirectoryGroupSearchResponse,
  DirectoryTreeResponse,
  DirectoryUserSearchResponse,
} from "../generated/auth-contracts";

export type {
  DirectoryGroup,
  DirectoryTreeNode,
  DirectoryTreeResponse,
  DirectoryUser,
} from "../generated/auth-contracts";

export interface DirectorySearchOptions {
  query?: string;
  limit?: number;
  cursor?: string;
}

export interface DirectoryTreeOptions {
  parent_id?: string | null;
  limit?: number;
  cursor?: string;
}

function buildQuery(params: Record<string, string | number | null | undefined>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value == null) continue;
    if (typeof value === "string") {
      if (value.trim() === "" && key !== "parent_id") continue;
      search.set(key, value);
      continue;
    }
    search.set(key, String(value));
  }
  const query = search.toString();
  return query ? `?${query}` : "";
}

export async function fetchDirectoryUsers(
  options: DirectorySearchOptions = {},
): Promise<DirectoryUserSearchResponse> {
  return api.get<DirectoryUserSearchResponse>(
    `/directory/users${buildQuery({
      query: options.query,
      limit: options.limit,
      cursor: options.cursor,
    })}`,
  );
}

export async function fetchDirectoryGroups(
  options: DirectorySearchOptions = {},
): Promise<DirectoryGroupSearchResponse> {
  return api.get<DirectoryGroupSearchResponse>(
    `/directory/groups${buildQuery({
      query: options.query,
      limit: options.limit,
      cursor: options.cursor,
    })}`,
  );
}

export async function fetchDirectoryGroupTree(
  options: DirectoryTreeOptions = {},
): Promise<DirectoryTreeResponse> {
  return api.get<DirectoryTreeResponse>(
    `/directory/groups/tree${buildQuery({
      parent_id: options.parent_id ?? "",
      limit: options.limit,
      cursor: options.cursor,
    })}`,
  );
}
