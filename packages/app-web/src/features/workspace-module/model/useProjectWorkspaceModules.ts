import { useEffect } from "react";

import { useWorkspaceModuleStore } from "./workspaceModuleStore";
import { idleProjectWorkspaceModulesState } from "./types";
import type { ProjectWorkspaceModulesState } from "./types";

export function useProjectWorkspaceModules(
  projectId: string | null,
): ProjectWorkspaceModulesState {
  const modulesState = useWorkspaceModuleStore((state) =>
    projectId ? state.byProjectId[projectId] : null,
  );
  const fetchProject = useWorkspaceModuleStore((state) => state.fetchProject);

  useEffect(() => {
    if (!projectId) return;
    const current = useWorkspaceModuleStore.getState().byProjectId[projectId];
    if (
      current?.status === "ready" ||
      current?.status === "loading" ||
      current?.status === "refreshing"
    ) {
      return;
    }
    void fetchProject(projectId);
  }, [fetchProject, projectId]);

  if (!projectId) return idleProjectWorkspaceModulesState();
  return (
    modulesState ?? {
      ...idleProjectWorkspaceModulesState(),
      project_id: projectId,
    }
  );
}
