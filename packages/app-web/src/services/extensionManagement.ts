import { api } from "../api/client";
import type { ProjectExtensionManagementListResponse } from "../generated/extension-management-contracts";

export async function fetchProjectExtensions(
  projectId: string,
): Promise<ProjectExtensionManagementListResponse> {
  return api.get<ProjectExtensionManagementListResponse>(
    `/projects/${encodeURIComponent(projectId)}/extensions`,
  );
}
