export interface WorkspaceModulePresentationPayload {
  module_id?: string | null;
  view_key: string;
  renderer_kind: string;
  presentation_uri?: string | null;
  title?: string | null;
  uri?: string | null;
}

export interface WorkspaceModulePresentedTabTarget {
  typeId: string;
  uri?: string;
  refreshRuntime: boolean;
}

const CANVAS_PRESENTATION_SCHEME = "canvas://";

export function isConcreteCanvasPresentationUri(uri: string): boolean {
  if (!uri.startsWith(CANVAS_PRESENTATION_SCHEME)) return false;
  return uri.slice(CANVAS_PRESENTATION_SCHEME.length).trim().length > 0;
}

export function workspaceModulePresentationTabTarget(
  data: WorkspaceModulePresentationPayload | null,
): WorkspaceModulePresentedTabTarget | null {
  if (!data) return null;
  const rendererKind = data.renderer_kind.trim();
  const viewKey = data.view_key.trim();
  const presentationUri = data.presentation_uri?.trim() ?? "";
  const fallbackUri = data.uri?.trim() ?? "";

  if (rendererKind === "canvas") {
    if (!isConcreteCanvasPresentationUri(presentationUri)) return null;
    return {
      typeId: "canvas",
      uri: presentationUri,
      refreshRuntime: true,
    };
  }

  if (!viewKey) return null;
  return {
    typeId: viewKey,
    uri: presentationUri || fallbackUri || undefined,
    refreshRuntime: false,
  };
}

export function workspaceModulePresentedTabTarget(
  data: Record<string, unknown> | null,
): WorkspaceModulePresentedTabTarget | null {
  const rendererKind = typeof data?.renderer_kind === "string" ? data.renderer_kind : "";
  const viewKey = typeof data?.view_key === "string" ? data.view_key : "";
  const presentationUri = typeof data?.presentation_uri === "string"
    ? data.presentation_uri
    : null;
  const fallbackUri = typeof data?.uri === "string" ? data.uri : null;
  const moduleId = typeof data?.module_id === "string" ? data.module_id : null;
  const title = typeof data?.title === "string" ? data.title : null;

  return workspaceModulePresentationTabTarget({
    module_id: moduleId,
    view_key: viewKey,
    renderer_kind: rendererKind,
    presentation_uri: presentationUri,
    title,
    uri: fallbackUri,
  });
}
