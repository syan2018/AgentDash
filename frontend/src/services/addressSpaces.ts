import { buildApiPath } from "../api/origin";

// ─── Descriptor（能力发现） ─────────────────────────────

export interface SelectorHint {
  trigger: string;
  placeholder: string;
  result_item_type: string;
}

export interface AddressSpaceDescriptor {
  id: string;
  label: string;
  kind: string;
  provider: string;
  supports: string[];
  root?: string | null;
  workspace_id?: string | null;
  selector?: SelectorHint | null;
}

export interface ListAddressSpacesResponse {
  spaces: AddressSpaceDescriptor[];
}

// ─── Entry（条目搜索） ──────────────────────────────────

export interface AddressEntry {
  address: string;
  label: string;
  entry_type: string;
}

export interface ListEntriesResponse {
  entries: AddressEntry[];
}

// ─── 查询参数 ────────────────────────────────────────────

export interface AddressSpaceQueryParams {
  workspaceId?: string | null;
}

export interface ListEntriesParams extends AddressSpaceQueryParams {
  query?: string;
}

// ─── API 函数 ────────────────────────────────────────────

function applyQueryParams(searchParams: URLSearchParams, params?: AddressSpaceQueryParams) {
  if (!params) return;
  if (params.workspaceId) searchParams.set("workspace_id", params.workspaceId);
}

export async function listAddressSpaces(
  params?: AddressSpaceQueryParams,
): Promise<ListAddressSpacesResponse> {
  const searchParams = new URLSearchParams();
  applyQueryParams(searchParams, params);

  const qs = searchParams.toString();
  const url = buildApiPath(`/address-spaces${qs ? `?${qs}` : ""}`);
  const res = await fetch(url);

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `listAddressSpaces failed: HTTP ${res.status}`);
  }

  return res.json();
}

export async function listAddressEntries(
  spaceId: string,
  params?: ListEntriesParams,
): Promise<ListEntriesResponse> {
  const searchParams = new URLSearchParams();
  applyQueryParams(searchParams, params);
  if (params?.query) searchParams.set("query", params.query);

  const qs = searchParams.toString();
  const url = buildApiPath(
    `/address-spaces/${encodeURIComponent(spaceId)}/entries${qs ? `?${qs}` : ""}`,
  );
  const res = await fetch(url);

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `listAddressEntries failed: HTTP ${res.status}`);
  }

  return res.json();
}
