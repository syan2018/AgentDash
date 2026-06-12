/* eslint-disable react-refresh/only-export-components */
import { useCallback } from "react";
import { CanvasRuntimePanel } from "../../canvas-panel";
import { useWorkspaceData } from "../workspace-data-context";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";
import type { TabContentRenderProps, TabTypeDescriptor } from "../tab-type-registry";
import { CanvasIcon } from "./icons";

const SCHEME = "canvas://";

function parseCanvasUri(uri: string): { canvasId: string } | null {
  if (!uri.startsWith(SCHEME)) return null;
  const canvasId = uri.slice(SCHEME.length);
  return canvasId ? { canvasId } : null;
}

function isConcreteCanvasUri(uri: string): boolean {
  return parseCanvasUri(uri) !== null;
}

function CanvasTabContent({ uri }: TabContentRenderProps) {
  const { sessionId } = useWorkspaceData();
  const parsed = parseCanvasUri(uri);
  const canvasId = parsed?.canvasId || null;

  const handleBrowseFiles = useCallback((mountId: string) => {
    const uri = `${mountId}://`;
    useWorkspaceTabStore.getState().openOrActivate("vfs", uri);
  }, []);

  if (!canvasId) {
    return (
      <div className="flex h-full min-h-[200px] flex-col items-center justify-center gap-3 px-6">
        <CanvasIcon className="h-8 w-8 text-muted-foreground/40" />
        <div className="text-center">
          <p className="text-sm font-medium text-muted-foreground">当前会话还没有关联的 Canvas</p>
          <p className="mt-1 text-xs text-muted-foreground/70">
            Canvas 展示会通过 workspace_module_present 打开具体视图
          </p>
        </div>
      </div>
    );
  }

  return (
    <CanvasRuntimePanel
      canvasId={canvasId}
      sessionId={sessionId}
      onClose={() => {}}
      onBrowseFiles={handleBrowseFiles}
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
    const shortId = parsed.canvasId.length > 8
      ? `${parsed.canvasId.slice(0, 8)}…`
      : parsed.canvasId;
    return `Canvas: ${shortId}`;
  },

  parseUri: (uri) => {
    const parsed = parseCanvasUri(uri);
    return parsed ? { canvasId: parsed.canvasId } : null;
  },
  canCreateUri: isConcreteCanvasUri,

  buildUri: (params) => {
    const canvasId = params?.canvasId;
    return canvasId ? `${SCHEME}${canvasId}` : "canvas://";
  },
  menuOrder: 10,
};
