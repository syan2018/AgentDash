import type {
  WorkspaceModuleDescriptor,
} from "../../../generated/workspace-module-contracts";
import {
  isConcreteCanvasPresentationUri,
  workspaceModulePresentationTabTarget,
} from "../../workspace-module/model/presentation";
import {
  presentAgentRunWorkspaceModule,
  presentWorkspaceModule,
} from "../../../services/workspaceModule";

export interface CanvasModuleOpenOption {
  module_id: string;
  view_key: string;
  title: string;
  presentation_uri: string;
}

export interface OpenUserCanvasModuleParams {
  option: CanvasModuleOpenOption;
  projectId: string;
  agentRunTarget?: { runId: string; agentId: string } | null;
  afterPresent?: () => Promise<void>;
  openOrActivate: (typeId: string, uri: string) => void;
}

export type CanvasSurfaceUri =
  | { kind: "definition"; definitionId: string }
  | { kind: "interaction"; instanceId: string };

export function parseCanvasSurfaceUri(uri: string): CanvasSurfaceUri | null {
  const trimmed = uri.trim();
  if (trimmed.startsWith("canvas://")) {
    const definitionId = trimmed.slice("canvas://".length).trim();
    if (!definitionId) return null;
    return {
      kind: "definition",
      definitionId,
    };
  }
  if (trimmed.startsWith("interaction://")) {
    const instanceId = trimmed.slice("interaction://".length).trim();
    return instanceId ? { kind: "interaction", instanceId } : null;
  }
  return null;
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
  option,
  projectId,
  agentRunTarget,
  afterPresent,
  openOrActivate,
}: OpenUserCanvasModuleParams): Promise<void> {
  const presentation = agentRunTarget
    ? await presentAgentRunWorkspaceModule(
      agentRunTarget.runId,
      agentRunTarget.agentId,
      {
        module_id: option.module_id,
        view_key: option.view_key,
      },
    )
    : await presentWorkspaceModule(projectId, {
      module_id: option.module_id,
      view_key: option.view_key,
    });
  const target = workspaceModulePresentationTabTarget(presentation);
  if (target?.typeId !== "canvas" || !target.uri) {
    throw new Error("当前 Canvas 没有可打开的 presentation。");
  }
  await afterPresent?.();
  openOrActivate(target.typeId, target.uri);
}
