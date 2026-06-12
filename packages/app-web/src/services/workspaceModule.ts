import { api } from "../api/client";
import type {
  WorkspaceModuleDescriptor,
  WorkspaceModulePresentRequest,
  WorkspaceModulePresentation,
} from "../generated/workspace-module-contracts";

/**
 * 拉取项目层 WorkspaceModule 合并认知（Canvas + Extension 贡献）。
 *
 * 复用后端 Child 1 的 canonical projection（`GET /projects/{id}/workspace-modules`），
 * 不引入第二份 DTO——返回类型即生成的 contract 类型。
 */
export async function fetchProjectWorkspaceModules(
  projectId: string,
): Promise<WorkspaceModuleDescriptor[]> {
  return api.get<WorkspaceModuleDescriptor[]>(
    `/projects/${encodeURIComponent(projectId)}/workspace-modules`,
  );
}

export async function presentWorkspaceModule(
  projectId: string,
  request: WorkspaceModulePresentRequest,
): Promise<WorkspaceModulePresentation> {
  return api.post<WorkspaceModulePresentation>(
    `/projects/${encodeURIComponent(projectId)}/workspace-modules/present`,
    request,
  );
}
