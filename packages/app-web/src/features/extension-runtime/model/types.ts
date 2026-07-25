import type { ExtensionRuntimeProjectionResponse } from "../../../types";
import type {
  ProjectExtensionRuntimeState,
  ProjectExtensionRuntimeStatus,
} from "../../workspace-runtime";

export type { ProjectExtensionRuntimeState, ProjectExtensionRuntimeStatus };

export function emptyExtensionRuntimeProjection(): ExtensionRuntimeProjectionResponse {
  return {
    installations: [],
    commands: [],
    flags: [],
    message_renderers: [],
    runtime_actions: [],
    protocols: [],
    extension_dependencies: [],
    workspace_tabs: [],
    ui_components: [],
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
