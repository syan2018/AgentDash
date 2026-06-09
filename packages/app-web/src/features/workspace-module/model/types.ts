import type { WorkspaceModuleDescriptor } from "../../../generated/workspace-module-contracts";

export type ProjectWorkspaceModulesStatus =
  | "idle"
  | "loading"
  | "ready"
  | "refreshing"
  | "error";

/**
 * 项目层 WorkspaceModule 合并认知状态切片。
 *
 * `modules` 是后端 canonical projection 的直投（Canvas + Extension 合并），
 * 不做二次 DTO 转换。
 */
export interface ProjectWorkspaceModulesState {
  project_id: string | null;
  status: ProjectWorkspaceModulesStatus;
  modules: WorkspaceModuleDescriptor[];
  error: string | null;
}

export function idleProjectWorkspaceModulesState(): ProjectWorkspaceModulesState {
  return {
    project_id: null,
    status: "idle",
    modules: [],
    error: null,
  };
}
