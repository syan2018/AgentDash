/* eslint-disable react-refresh/only-export-components */
import { CanvasRuntimePanel } from "../../canvas-panel";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";
import { useWorkspaceData } from "../workspace-data-context";
import { parseCanvasSurfaceUri } from "../model/canvasModuleOpen";
import type { TabContentRenderProps, TabTypeDescriptor } from "../tab-type-registry";
import { CanvasIcon } from "./icons";

function CanvasTabContent({ uri, refreshRevision }: TabContentRenderProps) {
  const { projectId } = useWorkspaceData();
  const parsed = parseCanvasSurfaceUri(uri);
  if (!parsed) {
    return (
      <div className="flex h-full min-h-[200px] flex-col items-center justify-center gap-3 px-6">
        <CanvasIcon className="h-8 w-8 text-muted-foreground/40" />
        <div className="text-center">
          <p className="text-sm font-medium text-muted-foreground">
            请选择具体 Canvas definition 或 Interaction instance
          </p>
        </div>
      </div>
    );
  }

  return (
    <CanvasRuntimePanel
      projectId={projectId}
      definitionId={parsed.kind === "definition" ? parsed.id : null}
      instanceId={parsed.kind === "interaction" ? parsed.id : null}
      refreshRevision={refreshRevision}
      onOpenInteraction={(instanceId) => {
        useWorkspaceTabStore.getState().openOrActivate("canvas", `interaction://${instanceId}`);
      }}
    />
  );
}

export const canvasTabType: TabTypeDescriptor = {
  typeId: "canvas",
  label: "Canvas",
  icon: CanvasIcon,
  allowMultiple: true,
  pinned: false,
  defaultUri: "canvas://",

  renderContent: (props) => <CanvasTabContent {...props} />,

  resolveTitle: (uri) => {
    const parsed = parseCanvasSurfaceUri(uri);
    if (!parsed) return "Canvas";
    const shortId = parsed.id.length > 8 ? `${parsed.id.slice(0, 8)}…` : parsed.id;
    return parsed.kind === "interaction" ? `Interaction: ${shortId}` : `Canvas: ${shortId}`;
  },

  parseUri: (uri) => {
    const parsed = parseCanvasSurfaceUri(uri);
    return parsed ? { kind: parsed.kind, id: parsed.id } : null;
  },
  canCreateUri: (uri) => parseCanvasSurfaceUri(uri) !== null,

  buildUri: (params) => {
    const id = params?.id;
    return id
      ? `${params.kind === "interaction" ? "interaction" : "canvas"}://${id}`
      : "canvas://";
  },
  menuOrder: 10,
};
