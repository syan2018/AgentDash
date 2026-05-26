import { useEffect, useMemo, useState } from "react";

import { authenticatedFetch } from "../../../api/client";
import { mapCanvasRuntimeSnapshot } from "../../../services/canvas";
import { buildExtensionWebviewAssetUrl } from "../../../services/extensionRuntime";
import type {
  CanvasRuntimeSnapshot,
  ExtensionWorkspaceTabProjectionResponse,
} from "../../../types";
import { CanvasRuntimePreview } from "../../canvas-panel/CanvasRuntimePreview";
import { useWorkspaceData, type WorkspaceData } from "../../workspace-panel/workspace-data-context";

interface ExtensionCanvasPanelProps {
  tab: ExtensionWorkspaceTabProjectionResponse;
}

interface Availability {
  available: boolean;
  title: string;
  detail: string;
  assetUrl: string | null;
}

type SnapshotState =
  | { status: "ready"; assetUrl: string; snapshot: CanvasRuntimeSnapshot }
  | { status: "error"; assetUrl: string; message: string };

export function ExtensionCanvasPanel({ tab }: ExtensionCanvasPanelProps) {
  const workspaceData = useWorkspaceData();
  const availability = useMemo(
    () => resolveAvailability(workspaceData, tab),
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
        <CanvasRuntimePreview snapshot={snapshot} />
      </div>
    );
  }

  return null;
}

function resolveAvailability(
  workspaceData: WorkspaceData,
  tab: ExtensionWorkspaceTabProjectionResponse,
): Availability {
  if (
    workspaceData.extensionRuntime.status === "loading"
    || workspaceData.extensionRuntime.status === "idle"
  ) {
    return unavailable("Extension runtime 正在加载", "Project extension runtime projection 尚未就绪。");
  }
  if (workspaceData.extensionRuntime.status === "error") {
    return unavailable(
      "Extension runtime 加载失败",
      workspaceData.extensionRuntime.error ?? "Project extension runtime projection 不可用。",
    );
  }
  if (!workspaceData.projectId) {
    return unavailable("Canvas extension 不可用", "当前页面缺少 Project context。");
  }
  const installation = workspaceData.extensionRuntime.projection.installations.find(
    (item) => item.extension_key === tab.extension_key,
  );
  if (!installation) {
    return unavailable("Extension 已停用", "当前 Project 没有启用这个插件。");
  }
  if (!installation.package_artifact) {
    return unavailable("Extension bundle 缺失", "当前插件安装没有可加载的 package artifact。");
  }
  if (tab.renderer.kind !== "canvas_panel") {
    return unavailable("Canvas renderer 不匹配", "当前插件 tab 不是 Canvas renderer。");
  }
  const entry = tab.renderer.entry.trim();
  if (!entry) {
    return unavailable("Canvas bundle 缺失", "插件 Canvas renderer 缺少 entry。");
  }

  return {
    available: true,
    title: "",
    detail: "",
    assetUrl: buildExtensionWebviewAssetUrl(workspaceData.projectId, tab.extension_key, entry),
  };
}

function unavailable(title: string, detail: string): Availability {
  return {
    available: false,
    title,
    detail,
    assetUrl: null,
  };
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
