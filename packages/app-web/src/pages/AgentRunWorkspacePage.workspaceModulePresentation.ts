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

export function workspaceModulePresentedTabTarget(
  data: Record<string, unknown> | null,
): WorkspaceModulePresentedTabTarget | null {
  const rendererKind = typeof data?.renderer_kind === "string" ? data.renderer_kind : "";
  const viewKey = typeof data?.view_key === "string" ? (data.view_key as string).trim() : "";
  const presentationUri = typeof data?.presentation_uri === "string"
    ? (data.presentation_uri as string).trim()
    : "";
  const fallbackUri = typeof data?.uri === "string" ? (data.uri as string).trim() : "";

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
