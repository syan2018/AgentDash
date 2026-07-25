/* eslint-disable react-refresh/only-export-components */
import { useCallback, useMemo } from "react";
import { CanvasRuntimePanel } from "../../canvas-panel";
import { useWorkspaceData } from "../workspace-data-context";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";
import type { TabContentRenderProps, TabTypeDescriptor } from "../tab-type-registry";
import { CanvasIcon } from "./icons";

const SCHEME = "canvas://";

function parseCanvasUri(uri: string): { canvasMountId: string } | null {
  if (!uri.startsWith(SCHEME)) return null;
  const canvasMountId = uri.slice(SCHEME.length);
  return canvasMountId ? { canvasMountId } : null;
}

function isConcreteCanvasUri(uri: string): boolean {
  return parseCanvasUri(uri) !== null;
}

function CanvasTabContent({ uri, refreshRevision }: TabContentRenderProps) {
  const {
    projectId,
    agentRunCanvasBridgeBase,
  } = useWorkspaceData();
  const parsed = parseCanvasUri(uri);
  const canvasMountId = parsed?.canvasMountId || null;
  const bridgeRunId = agentRunCanvasBridgeBase?.run_id ?? null;
  const bridgeAgentId = agentRunCanvasBridgeBase?.agent_id ?? null;
  const bridgeProjectId = agentRunCanvasBridgeBase?.project_id ?? null;
  const agentRunBridge = useMemo(
    () => bridgeRunId && bridgeAgentId && bridgeProjectId && canvasMountId
      ? {
          run_id: bridgeRunId,
          agent_id: bridgeAgentId,
          project_id: bridgeProjectId,
          canvas_mount_id: canvasMountId,
        }
      : null,
    [
      bridgeAgentId,
      bridgeProjectId,
      bridgeRunId,
      canvasMountId,
    ],
  );

  const handleBrowseFiles = useCallback((mountId: string) => {
    const uri = `${mountId}://`;
    useWorkspaceTabStore.getState().openOrActivate("vfs", uri);
  }, []);

  if (!canvasMountId) {
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
      canvasId={null}
      canvasMountId={canvasMountId}
      projectId={projectId}
      agentRunBridge={agentRunBridge}
      showBridgeUnavailable={agentRunCanvasBridgeBase === null}
      refreshRevision={refreshRevision}
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
    const shortId = parsed.canvasMountId.length > 8
      ? `${parsed.canvasMountId.slice(0, 8)}…`
      : parsed.canvasMountId;
    return `Canvas: ${shortId}`;
  },

  parseUri: (uri) => {
    const parsed = parseCanvasUri(uri);
    return parsed ? { canvasMountId: parsed.canvasMountId } : null;
  },
  canCreateUri: isConcreteCanvasUri,

  buildUri: (params) => {
    const canvasMountId = params?.canvasMountId;
    return canvasMountId ? `${SCHEME}${canvasMountId}` : "canvas://";
  },
  menuOrder: 10,
};
