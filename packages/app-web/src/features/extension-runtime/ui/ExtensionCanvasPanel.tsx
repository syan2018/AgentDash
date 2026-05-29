import { useEffect, useMemo, useState } from "react";

import { authenticatedFetch } from "../../../api/client";
import { mapCanvasRuntimeSnapshot } from "../../../services/canvas";
import { invokeProjectExtensionRuntimeChannel } from "../../../services/extensionRuntime";
import type {
  CanvasRuntimeSnapshot,
  ExtensionWorkspaceTabProjectionResponse,
} from "../../../types";
import { CanvasRuntimePreview } from "../../canvas-panel/CanvasRuntimePreview";
import { useWorkspaceData } from "../../workspace-runtime";
import {
  invokeExtensionChannelFromCanvas,
  resolveExtensionCanvasAvailability,
} from "../model/canvasBridge";

interface ExtensionCanvasPanelProps {
  tab: ExtensionWorkspaceTabProjectionResponse;
}

type SnapshotState =
  | { status: "ready"; assetUrl: string; snapshot: CanvasRuntimeSnapshot }
  | { status: "error"; assetUrl: string; message: string };

export function ExtensionCanvasPanel({ tab }: ExtensionCanvasPanelProps) {
  const workspaceData = useWorkspaceData();
  const availability = useMemo(
    () => resolveExtensionCanvasAvailability(workspaceData, tab),
    [workspaceData, tab],
  );
  const isAvailable = availability.available;
  const assetUrl = availability.assetUrl;
  const [snapshotState, setSnapshotState] = useState<SnapshotState | null>(null);

  useEffect(() => {
    if (!isAvailable || !assetUrl) {
      return;
    }
    let cancelled = false;
    authenticatedFetch(assetUrl)
      .then(async (response) => {
        if (!response.ok) {
          throw new Error(`Canvas package snapshot 加载失败: HTTP ${response.status}`);
        }
        return response.json() as Promise<Record<string, unknown>>;
      })
      .then((raw) => {
        if (cancelled) return;
        const snapshot = mapCanvasRuntimeSnapshot(raw);
        setSnapshotState({
          status: "ready",
          assetUrl,
          snapshot,
        });
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setSnapshotState({
          status: "error",
          assetUrl,
          message: error instanceof Error ? error.message : "Canvas package snapshot 加载失败",
        });
      });

    return () => {
      cancelled = true;
    };
  }, [assetUrl, isAvailable]);

  if (!availability.available) {
    return (
      <ExtensionCanvasUnavailableState
        title={availability.title}
        detail={availability.detail}
      />
    );
  }

  if (!snapshotState || snapshotState.assetUrl !== assetUrl) {
    return (
      <div className="flex h-full min-h-[180px] items-center justify-center bg-background p-6 text-sm text-muted-foreground">
        正在加载 Canvas package...
      </div>
    );
  }

  if (snapshotState.status === "error") {
    return (
      <ExtensionCanvasUnavailableState
        title="Canvas package 加载失败"
        detail={snapshotState.message}
      />
    );
  }

  if (snapshotState.status === "ready") {
    const snapshot = {
      ...snapshotState.snapshot,
      session_id: workspaceData.sessionId ?? snapshotState.snapshot.session_id ?? null,
    };
    return (
      <div className="flex h-full min-h-0 bg-background">
        <CanvasRuntimePreview
          snapshot={snapshot}
          extensionChannelBridge={(request) =>
            invokeExtensionChannelFromCanvas({
              workspaceData,
              tab,
              request,
              invokeChannel: invokeProjectExtensionRuntimeChannel,
            })}
        />
      </div>
    );
  }

  return null;
}

function ExtensionCanvasUnavailableState({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="flex h-full min-h-[180px] items-center justify-center bg-background p-6">
      <div className="max-w-sm rounded-[8px] border border-border bg-secondary/25 px-4 py-3 text-sm">
        <p className="font-medium text-foreground">{title}</p>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{detail}</p>
      </div>
    </div>
  );
}
