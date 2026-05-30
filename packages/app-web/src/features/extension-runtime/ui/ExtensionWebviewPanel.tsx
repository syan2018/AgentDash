import { useCallback, useEffect, useMemo, useRef } from "react";

import {
  invokeProjectExtensionRuntimeAction,
  invokeProjectExtensionRuntimeChannel,
} from "../../../services/extensionRuntime";
import { readSurfaceFile, writeSurfaceFile } from "../../../services/vfs";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";
import type { ExtensionWorkspaceTabProjectionResponse } from "../../../types";
import { useWorkspaceData } from "../../workspace-runtime";
import {
  parseExtensionBridgeMessage,
  EXTENSION_BRIDGE_CHANNEL,
  type ExtensionBridgeRequestMessage,
} from "../model/bridge";
import {
  handleExtensionWebviewBridgeRequest,
  resolveExtensionWebviewAvailability,
  type ExtensionWebviewBridgeServices,
} from "../model/webviewBridge";

interface ExtensionWebviewPanelProps {
  tab: ExtensionWorkspaceTabProjectionResponse;
  uri: string;
  tabId: string;
  isActive: boolean;
}

const webviewBridgeServices: ExtensionWebviewBridgeServices = {
  openTab(typeId, uri) {
    useWorkspaceTabStore.getState().openOrActivate(typeId, uri);
  },
  invokeAction: invokeProjectExtensionRuntimeAction,
  invokeChannel: invokeProjectExtensionRuntimeChannel,
  readFile: readSurfaceFile,
  writeFile: writeSurfaceFile,
};

export function ExtensionWebviewPanel({
  tab,
  uri,
  tabId,
}: ExtensionWebviewPanelProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const workspaceData = useWorkspaceData();

  const availability = useMemo(
    () => resolveExtensionWebviewAvailability(workspaceData, tab),
    [workspaceData, tab],
  );

  const handleBridgeRequest = useCallback(
    (message: ExtensionBridgeRequestMessage) => handleExtensionWebviewBridgeRequest({
      message,
      workspaceData,
      tab,
      uri,
      backend: availability.backend,
      services: webviewBridgeServices,
    }),
    [availability.backend, tab, uri, workspaceData],
  );

  useEffect(() => {
    if (!availability.available) return;
    const frameWindow = iframeRef.current?.contentWindow;
    if (!frameWindow) return;

    const handleMessage = (event: MessageEvent<unknown>) => {
      if (event.source !== frameWindow) return;
      if (!isAllowedBridgeOrigin(event.origin)) return;
      const message = parseExtensionBridgeMessage(event.data);
      if (!message) return;
      if (message.kind === "event") return;

      void handleBridgeRequest(message)
        .then((result) => {
          frameWindow.postMessage({
            channel: EXTENSION_BRIDGE_CHANNEL,
            kind: "response",
            request_id: message.request_id,
            result,
          }, "*");
        })
        .catch((error: unknown) => {
          frameWindow.postMessage({
            channel: EXTENSION_BRIDGE_CHANNEL,
            kind: "response",
            request_id: message.request_id,
            error: error instanceof Error ? error.message : "Extension bridge 请求失败",
          }, "*");
        });
    };

    window.addEventListener("message", handleMessage);
    return () => window.removeEventListener("message", handleMessage);
  }, [availability.available, handleBridgeRequest]);

  if (!availability.available) {
    return (
      <ExtensionUnavailableState
        title={availability.title}
        detail={availability.detail}
      />
    );
  }

  return (
    <div className="h-full min-h-0 bg-background">
      <iframe
        ref={iframeRef}
        title={tab.label}
        src={availability.src ?? undefined}
        sandbox="allow-scripts"
        referrerPolicy="no-referrer"
        className="h-full w-full border-0 bg-background"
        data-extension-key={tab.extension_key}
        data-extension-tab-id={tabId}
      />
    </div>
  );
}

function isAllowedBridgeOrigin(origin: string): boolean {
  return origin === "null" || origin === window.location.origin;
}

function ExtensionUnavailableState({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="flex h-full min-h-[180px] items-center justify-center bg-background p-6">
      <div className="max-w-sm rounded-[8px] border border-border bg-secondary/25 px-4 py-3 text-sm">
        <p className="font-medium text-foreground">{title}</p>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{detail}</p>
      </div>
    </div>
  );
}
