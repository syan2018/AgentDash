import type {
  WorkspaceModuleDescriptor,
  WorkspaceModulePresentation,
  WorkspaceModulePresentRequest,
} from "../../../generated/workspace-module-contracts";
import {
  isConcreteCanvasPresentationUri,
  workspaceModulePresentationTabTarget,
} from "../../workspace-module/model/presentation";

export interface CanvasModuleOpenOption {
  module_id: string;
  view_key: string;
  title: string;
  presentation_uri: string;
}

export interface OpenUserCanvasModuleParams {
  projectId: string | null;
  runtimeSessionId: string | null;
  option: CanvasModuleOpenOption;
  presentWorkspaceModule: (
    projectId: string,
    request: WorkspaceModulePresentRequest,
  ) => Promise<WorkspaceModulePresentation>;
  openOrActivate: (typeId: string, uri: string) => void;
}

export function selectCanvasModuleOpenOptions(
  modules: WorkspaceModuleDescriptor[],
): CanvasModuleOpenOption[] {
  const options: CanvasModuleOpenOption[] = [];
  for (const module of modules) {
    if (module.summary.kind !== "canvas") continue;
    if (module.summary.status.kind !== "ready") continue;
    for (const entry of module.ui_entries) {
      if (entry.renderer_kind !== "canvas") continue;
      const presentationUri = entry.presentation_uri?.trim() ?? "";
      if (!isConcreteCanvasPresentationUri(presentationUri)) continue;
      const title = entry.title.trim() || module.summary.title.trim() || module.summary.module_id;
      options.push({
        module_id: module.summary.module_id,
        view_key: entry.view_key,
        title,
        presentation_uri: presentationUri,
      });
    }
  }
  return options;
}

export async function openUserCanvasModule({
  projectId,
  runtimeSessionId,
  option,
  presentWorkspaceModule,
  openOrActivate,
}: OpenUserCanvasModuleParams): Promise<void> {
  if (!projectId || !runtimeSessionId) {
    throw new Error("当前 AgentRun 尚未就绪，无法打开 Canvas。");
  }

  const presentation = await presentWorkspaceModule(projectId, {
    module_id: option.module_id,
    view_key: option.view_key,
    runtime_session_id: runtimeSessionId,
  });
  const target = workspaceModulePresentationTabTarget(presentation);
  if (target?.typeId !== "canvas" || !target.uri) {
    throw new Error("后端未返回可打开的 Canvas presentation。");
  }
  openOrActivate(target.typeId, target.uri);
}
