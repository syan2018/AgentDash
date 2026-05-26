import { useEffect } from "react";

import { useExtensionRuntimeStore } from "./extensionRuntimeStore";
import { idleProjectExtensionRuntimeState } from "./types";
import type { ProjectExtensionRuntimeState } from "./types";

export function useProjectExtensionRuntime(
  projectId: string | null,
): ProjectExtensionRuntimeState {
  const runtimeState = useExtensionRuntimeStore((state) => (
    projectId ? state.byProjectId[projectId] : null
  ));
  const fetchProject = useExtensionRuntimeStore((state) => state.fetchProject);

  useEffect(() => {
    if (!projectId) return;
    const current = useExtensionRuntimeStore.getState().byProjectId[projectId];
    if (current?.status === "ready" || current?.status === "loading" || current?.status === "refreshing") {
      return;
    }
    void fetchProject(projectId);
  }, [fetchProject, projectId]);

  if (!projectId) return idleProjectExtensionRuntimeState();
  return runtimeState ?? {
    ...idleProjectExtensionRuntimeState(),
    project_id: projectId,
  };
}
