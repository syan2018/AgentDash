/* eslint-disable react-refresh/only-export-components */
import { CanvasSessionPanel } from "../../canvas-panel";
import { useWorkspaceData } from "../workspace-data-context";
import type { TabContentRenderProps, TabTypeDescriptor } from "../tab-type-registry";
import { CanvasIcon } from "./icons";

const SCHEME = "canvas://";

function parseCanvasUri(uri: string): { canvasId: string } | null {
  if (!uri.startsWith(SCHEME)) return null;
  const canvasId = uri.slice(SCHEME.length);
  return canvasId ? { canvasId } : null;
}

function CanvasTabContent({ uri }: TabContentRenderProps) {
  const { sessionId } = useWorkspaceData();
  const parsed = parseCanvasUri(uri);
  const canvasId = parsed?.canvasId ?? null;

  if (!canvasId) {
    return (
      <div className="flex h-full min-h-[200px] items-center justify-center px-6">
        <p className="text-center text-sm text-muted-foreground">
          当前会话还没有关联的 Canvas。
        </p>
      </div>
    );
  }

  return (
    <CanvasSessionPanel
      canvasId={canvasId}
      sessionId={sessionId}
      onClose={() => {}}
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

  buildUri: (params) => {
    const canvasId = params?.canvasId;
    return canvasId ? `${SCHEME}${canvasId}` : "canvas://";
  },
  menuOrder: 10,
};
