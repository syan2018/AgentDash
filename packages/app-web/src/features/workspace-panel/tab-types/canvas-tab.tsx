/* eslint-disable react-refresh/only-export-components */
import { CanvasRuntimePanel } from "../../canvas-panel";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";
import { useWorkspaceData } from "../workspace-data-context";
import type { TabContentRenderProps, TabTypeDescriptor } from "../tab-type-registry";
import { CanvasIcon } from "./icons";

type ParsedCanvasUri =
  | { kind: "definition"; id: string }
  | { kind: "interaction"; id: string };

function parseCanvasUri(uri: string): ParsedCanvasUri | null {
  if (uri.startsWith("canvas://")) {
    const id = uri.slice("canvas://".length).trim();
    return id ? { kind: "definition", id } : null;
  }
  if (uri.startsWith("interaction://")) {
    const id = uri.slice("interaction://".length).trim();
    return id ? { kind: "interaction", id } : null;
  }
  return null;
}

function CanvasTabContent({ uri, refreshRevision }: TabContentRenderProps) {
  const { projectId } = useWorkspaceData();
  const parsed = parseCanvasUri(uri);
  if (!parsed) {
    return <div className="p-6 text-sm text-muted-foreground">请选择具体 Canvas definition 或 Interaction instance。</div>;
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
    const parsed = parseCanvasUri(uri);
    if (!parsed) return "Canvas";
    const shortId = parsed.id.length > 8 ? `${parsed.id.slice(0, 8)}…` : parsed.id;
    return parsed.kind === "interaction" ? `Interaction: ${shortId}` : `Canvas: ${shortId}`;
  },
  parseUri: (uri) => {
    const parsed = parseCanvasUri(uri);
    return parsed ? { kind: parsed.kind, id: parsed.id } : null;
  },
  canCreateUri: (uri) => parseCanvasUri(uri) !== null,
  buildUri: (params) => params.id
    ? `${params.kind === "interaction" ? "interaction" : "canvas"}://${params.id}`
    : "canvas://",
  menuOrder: 10,
};
