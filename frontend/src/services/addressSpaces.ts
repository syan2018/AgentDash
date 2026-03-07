import { buildApiPath } from "../api/origin";
import type { FileEntry } from "./workspaceFiles";

export interface AddressSpaceSelector {
  trigger: string;
  placeholder: string;
  resultItemType: string;
}

export interface AddressSpaceDescriptor {
  id: string;
  label: string;
  kind: string;
  provider: string;
  supports: string[];
  root?: string | null;
  workspaceId?: string | null;
  selector?: AddressSpaceSelector | null;
}

export interface ListAddressSpacesResponse {
  spaces: AddressSpaceDescriptor[];
}

export interface ListAddressEntriesParams {
  projectId?: string | null;
  storyId?: string | null;
  taskId?: string | null;
  workspaceId?: string | null;
  query?: string;
}

export interface ListAddressEntriesResponse {
  spaceId: string;
  root: string;
  workspaceId: string;
  entries: FileEntry[];
}

function appendAddressSpaceParams(
  searchParams: URLSearchParams,
  params?: Omit<ListAddressEntriesParams, "query">,
) {
  if (!params) return;
  if (params.projectId) searchParams.set("projectId", params.projectId);
  if (params.storyId) searchParams.set("storyId", params.storyId);
  if (params.taskId) searchParams.set("taskId", params.taskId);
  if (params.workspaceId) searchParams.set("workspaceId", params.workspaceId);
}

export async function listAddressSpaces(
  params?: Omit<ListAddressEntriesParams, "query">,
): Promise<ListAddressSpacesResponse> {
  const searchParams = new URLSearchParams();
  appendAddressSpaceParams(searchParams, params);

  const url = buildApiPath(`/address-spaces${searchParams.toString() ? `?${searchParams}` : ""}`);
  const res = await fetch(url);

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `listAddressSpaces failed: HTTP ${res.status}`);
  }

  return res.json();
}

export async function listAddressEntries(
  spaceId: string,
  params?: ListAddressEntriesParams,
): Promise<ListAddressEntriesResponse> {
  const searchParams = new URLSearchParams();
  appendAddressSpaceParams(searchParams, params);
  if (params?.query) searchParams.set("query", params.query);

  const url = buildApiPath(
    `/address-spaces/${encodeURIComponent(spaceId)}/entries${searchParams.toString() ? `?${searchParams}` : ""}`,
  );
  const res = await fetch(url);

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `listAddressEntries failed: HTTP ${res.status}`);
  }

  return res.json();
}
