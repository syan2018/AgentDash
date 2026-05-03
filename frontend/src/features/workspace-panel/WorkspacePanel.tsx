import { forwardRef, useImperativeHandle } from "react";
import { CanvasSessionPanel } from "../canvas-panel";
import { ContextInspectorPanel } from "../session-context";
import { VfsBrowser } from "../vfs";
import { ContextOverviewTab } from "./ContextOverviewTab";
import type {
  WorkspacePanelHandle,
  WorkspacePanelProps,
  WorkspacePanelTab,
} from "./workspace-panel-types";

const TAB_ITEMS: { id: WorkspacePanelTab; label: string }[] = [
  { id: "context", label: "上下文" },
  { id: "vfs", label: "地址空间" },
  { id: "canvas", label: "Canvas" },
  { id: "inspector", label: "审计" },
];

export const WorkspacePanel = forwardRef<WorkspacePanelHandle, WorkspacePanelProps>(
  function WorkspacePanel(props, ref) {
    const {
      sessionId,
      contextSnapshot,
      ownerStory,
      ownerProjectName,
      executorSummary,
      runtimeSurface,
      vfs,
      hookRuntime,
      sessionCapabilities,
      activeCanvasId,
      activeTab,
      onTabChange,
    } = props;

    useImperativeHandle(ref, () => ({
      openTab: (tab: WorkspacePanelTab) => onTabChange(tab),
    }), [onTabChange]);

    return (
      <div className="flex h-full flex-col overflow-hidden bg-background">
        {/* Tab 栏 */}
        <div className="flex shrink-0 items-center gap-0.5 border-b border-border bg-secondary/20 px-2 py-1.5">
          {TAB_ITEMS.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => onTabChange(item.id)}
              className={[
                "rounded-[8px] px-3 py-1.5 text-xs font-medium transition-colors",
                activeTab === item.id
                  ? "bg-background text-foreground shadow-sm"
                  : "text-muted-foreground hover:bg-background/60 hover:text-foreground",
              ].join(" ")}
            >
              {item.label}
            </button>
          ))}
        </div>

        {/* Tab 内容区 */}
        <div className="min-h-0 flex-1 overflow-y-auto">
          {activeTab === "context" && (
            <ContextOverviewTab
              contextSnapshot={contextSnapshot}
              ownerStory={ownerStory}
              ownerProjectName={ownerProjectName}
              executorSummary={executorSummary}
              runtimeSurface={runtimeSurface}
              vfs={vfs}
              hookRuntime={hookRuntime}
              sessionCapabilities={sessionCapabilities}
            />
          )}

          {activeTab === "vfs" && (
            <VfsTab vfs={vfs} runtimeSurface={runtimeSurface} />
          )}

          {activeTab === "canvas" && (
            activeCanvasId ? (
              <CanvasSessionPanel
                canvasId={activeCanvasId}
                sessionId={sessionId}
                onClose={() => onTabChange("context")}
              />
            ) : (
              <EmptyTabPlaceholder message="当前会话还没有关联的 Canvas。" />
            )
          )}

          {activeTab === "inspector" && (
            sessionId ? (
              <ContextInspectorPanel sessionId={sessionId} />
            ) : (
              <EmptyTabPlaceholder message="需要先建立会话才能查看上下文审计。" />
            )
          )}
        </div>
      </div>
    );
  },
);

// ─── VFS Tab ────────────────────────────────────────────

function VfsTab({
  vfs,
  runtimeSurface,
}: {
  vfs: WorkspacePanelProps["vfs"];
  runtimeSurface: WorkspacePanelProps["runtimeSurface"];
}) {
  const hasMounts = (vfs && vfs.mounts.length > 0) || (runtimeSurface && runtimeSurface.mounts.length > 0);

  if (!hasMounts) {
    return <EmptyTabPlaceholder message="当前会话没有挂载的地址空间。" />;
  }

  return (
    <div className="p-4">
      <VfsBrowser surface={runtimeSurface} vfs={vfs} />
    </div>
  );
}

// ─── 空占位 ─────────────────────────────────────────────

function EmptyTabPlaceholder({ message }: { message: string }) {
  return (
    <div className="flex h-full min-h-[200px] items-center justify-center px-6">
      <p className="text-center text-sm text-muted-foreground">{message}</p>
    </div>
  );
}
