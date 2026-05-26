import type { ExtensionRuntimeProjectionResponse } from "../../../types";

export type ProjectExtensionRuntimeStatus = "idle" | "loading" | "ready" | "refreshing" | "error";

export interface ProjectExtensionRuntimeState {
  project_id: string | null;
  status: ProjectExtensionRuntimeStatus;
  projection: ExtensionRuntimeProjectionResponse;
  error: string | null;
}

export function emptyExtensionRuntimeProjection(): ExtensionRuntimeProjectionResponse {
  return {
    installations: [],
    commands: [],
    flags: [],
    message_renderers: [],
    runtime_actions: [],
    workspace_tabs: [],
    permissions: [],
    bundles: [],
  };
}

export function idleProjectExtensionRuntimeState(): ProjectExtensionRuntimeState {
  return {
    project_id: null,
    status: "idle",
    projection: emptyExtensionRuntimeProjection(),
    error: null,
  };
}
