/* eslint-disable react-refresh/only-export-components */
import { useCallback } from "react";
import { CanvasRuntimePanel } from "../../canvas-panel";
import { useWorkspaceData } from "../workspace-data-context";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";
import type { TabContentRenderProps, TabTypeDescriptor } from "../tab-type-registry";
import { CanvasIcon } from "./icons";
import { parseCanvasSurfaceUri } from "../model/canvasModuleOpen";

const SCHEME = "canvas://";

function isConcreteCanvasUri(uri: string): boolean {
  return parseCanvasSurfaceUri(uri) !== null;
}

function CanvasTabContent({ uri, refreshRevision }: TabContentRenderProps) {
  const { agentRunRuntimeTarget, projectId, runtimeSurface } = useWorkspaceData();
  const parsed = parseCanvasSurfaceUri(uri);
  const canvasId = parsed?.kind === "definition" ? parsed.definitionId : null;
  const interactionInstanceId = parsed?.kind === "interaction" ? parsed.instanceId : null;
  const workspaceKey = agentRunRuntimeTarget
    ? `agentrun:${agentRunRuntimeTarget.runId}:${agentRunRuntimeTarget.agentId}`
    : null;

  const handleBrowseFiles = useCallback((mountId: string) => {
    const uri = `${mountId}://`;
    useWorkspaceTabStore
      .getState()
      .openOrActivateInWorkspace(workspaceKey, "vfs", uri);
  }, [workspaceKey]);
  const canBrowseFiles = useCallback(
    (mountId: string) => Boolean(
      runtimeSurface?.mounts.some(
        (mount) => mount.id === mountId && mount.provider === "canvas_fs",
      ),
    ),
    [runtimeSurface?.mounts],
  );

  if (!parsed) {
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
      interactionInstanceId={interactionInstanceId}
      projectId={projectId}
      resourceSurfaceRef={runtimeSurface?.surface_ref}
      agentRunTarget={agentRunRuntimeTarget}
      refreshRevision={refreshRevision}
      onClose={() => {}}
      onBrowseFiles={handleBrowseFiles}
      canBrowseFiles={canBrowseFiles}
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
    const id = parsed.kind === "definition" ? parsed.definitionId : parsed.instanceId;
    const shortId = id.length > 8 ? `${id.slice(0, 8)}…` : id;
    return `Canvas: ${shortId}`;
  },

  parseUri: (uri) => {
    return parseCanvasSurfaceUri(uri);
  },
  canCreateUri: isConcreteCanvasUri,

  buildUri: (params) => {
    const canvasMountId = params?.canvasMountId;
    return canvasMountId ? `${SCHEME}${canvasMountId}` : "canvas://";
  },
  menuOrder: 10,
};
