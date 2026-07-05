import type {
  WorkspaceModuleDescriptor,
} from "../../../generated/workspace-module-contracts";
import type { ResolvedVfsSurface } from "../../../generated/vfs-contracts";
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
  option: CanvasModuleOpenOption;
  openOrActivate: (typeId: string, uri: string, refreshContent?: boolean) => void;
}

export function canvasMountIdFromPresentationUri(uri: string): string | null {
  const trimmed = uri.trim();
  if (!isConcreteCanvasPresentationUri(trimmed)) return null;
  return trimmed.slice("canvas://".length).trim() || null;
}

export function activeCanvasMountIdsFromRuntimeSurface(
  runtimeSurface: ResolvedVfsSurface | null,
): Set<string> {
  const ids = new Set<string>();
  for (const mount of runtimeSurface?.mounts ?? []) {
    if (mount.purpose !== "canvas") continue;
    const rawId = mount.id.trim();
    if (!rawId) continue;
    ids.add(rawId);
  }
  return ids;
}

export function isActiveCanvasPresentationUri(
  uri: string,
  activeCanvasMountIds: ReadonlySet<string>,
): boolean {
  const mountId = canvasMountIdFromPresentationUri(uri);
  return mountId !== null && activeCanvasMountIds.has(mountId);
}

export function selectCanvasModuleOpenOptions(
  modules: WorkspaceModuleDescriptor[],
  activeCanvasMountIds?: ReadonlySet<string> | null,
): CanvasModuleOpenOption[] {
  const options: CanvasModuleOpenOption[] = [];
  for (const module of modules) {
    if (module.summary.kind !== "canvas") continue;
    if (module.summary.status.kind !== "ready") continue;
    for (const entry of module.ui_entries) {
      if (entry.renderer_kind !== "canvas") continue;
      const presentationUri = entry.presentation_uri?.trim() ?? "";
      if (!isConcreteCanvasPresentationUri(presentationUri)) continue;
      const mountId = canvasMountIdFromPresentationUri(presentationUri);
      if (activeCanvasMountIds === null) continue;
      if (activeCanvasMountIds && (!mountId || !activeCanvasMountIds.has(mountId))) {
        continue;
      }
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
  openOrActivate,
}: OpenUserCanvasModuleParams): Promise<void> {
  const target = workspaceModulePresentationTabTarget({
    module_id: option.module_id,
    view_key: option.view_key,
    renderer_kind: "canvas",
    presentation_uri: option.presentation_uri,
    title: option.title,
  });
  if (target?.typeId !== "canvas" || !target.uri) {
    throw new Error("当前 Canvas 没有可打开的 presentation。");
  }
  openOrActivate(target.typeId, target.uri, target.refreshRuntime);
}
