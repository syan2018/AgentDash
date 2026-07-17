import type { JsonValue } from "../../../generated/common-contracts";
import type {
  WorkspaceModuleDescriptor,
  WorkspaceModulePresentation,
} from "../../../generated/workspace-module-contracts";

export interface WorkspaceModuleTabTarget {
  typeId: string;
  uri?: string;
}

const CANVAS_PRESENTATION_SCHEME = "canvas://";

export function isConcreteCanvasPresentationUri(uri: string): boolean {
  if (!uri.startsWith(CANVAS_PRESENTATION_SCHEME)) return false;
  return uri.slice(CANVAS_PRESENTATION_SCHEME.length).trim().length > 0;
}

function isJsonRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function isJsonValue(value: unknown): value is JsonValue {
  if (value == null) return true;
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return true;
  }
  if (Array.isArray(value)) return value.every(isJsonValue);
  if (!isJsonRecord(value)) return false;
  return Object.values(value).every(isJsonValue);
}

export function isWorkspaceModulePresentation(value: unknown): value is WorkspaceModulePresentation {
  if (!isJsonRecord(value)) return false;
  return (
    typeof value.module_id === "string" &&
    typeof value.view_key === "string" &&
    typeof value.renderer_kind === "string" &&
    typeof value.presentation_uri === "string" &&
    typeof value.title === "string" &&
    (value.payload === undefined || isJsonValue(value.payload)) &&
    (value.diagnostics === undefined || isJsonValue(value.diagnostics))
  );
}

export function workspaceModulePresentationFromPlatformEventData(
  data: unknown,
): WorkspaceModulePresentation | null {
  return isWorkspaceModulePresentation(data) ? data : null;
}

export function workspaceModulePresentationTabTarget(
  data: WorkspaceModulePresentation | null,
): WorkspaceModuleTabTarget | null {
  if (!data) return null;
  const rendererKind = data.renderer_kind.trim();
  const viewKey = data.view_key.trim();
  const presentationUri = data.presentation_uri.trim();

  if (rendererKind === "canvas") {
    if (!isConcreteCanvasPresentationUri(presentationUri)) return null;
    return {
      typeId: "canvas",
      uri: presentationUri,
    };
  }

  if (!viewKey) return null;
  return {
    typeId: viewKey,
    uri: presentationUri || undefined,
  };
}

export function isWorkspaceModulePresentationCurrent(
  presentation: WorkspaceModulePresentation,
  modules: readonly WorkspaceModuleDescriptor[],
): boolean {
  const moduleId = presentation.module_id.trim();
  const viewKey = presentation.view_key.trim();
  const rendererKind = presentation.renderer_kind.trim();
  const presentationUri = presentation.presentation_uri.trim();
  if (!moduleId || !viewKey || !rendererKind) return false;

  const module = modules.find((candidate) =>
    candidate.summary.module_id === moduleId
    && candidate.summary.status.kind === "ready"
  );
  if (!module) return false;
  return module.ui_entries.some((entry) =>
    entry.view_key === viewKey
    && entry.renderer_kind === rendererKind
    && (entry.presentation_uri?.trim() ?? "") === presentationUri
  );
}
