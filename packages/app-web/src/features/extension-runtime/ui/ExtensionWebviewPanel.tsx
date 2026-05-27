import { useCallback, useEffect, useMemo, useRef } from "react";

import {
  buildExtensionWebviewAssetUrl,
  invokeProjectExtensionRuntimeAction,
  invokeProjectExtensionRuntimeChannel,
} from "../../../services/extensionRuntime";
import { readSurfaceFile, writeSurfaceFile } from "../../../services/vfs";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";
import type { JsonValue } from "../../../generated/extension-runtime-contracts";
import type {
  ExtensionWorkspaceTabProjectionResponse,
  ResolvedMountSummary,
} from "../../../types";
import { useWorkspaceData, type WorkspaceData } from "../../workspace-panel/workspace-data-context";
import type { WorkspaceBackendTarget } from "../../workspace-panel/workspace-panel-types";
import {
  bridgeParamString,
  parseExtensionBridgeMessage,
  toJsonValue,
  EXTENSION_BRIDGE_CHANNEL,
  type ExtensionBridgeRequestMessage,
} from "../model/bridge";

interface ExtensionWebviewPanelProps {
  tab: ExtensionWorkspaceTabProjectionResponse;
  uri: string;
  tabId: string;
  isActive: boolean;
}

type BackendTarget = WorkspaceBackendTarget;

interface Availability {
  available: boolean;
  title: string;
  detail: string;
  src: string | null;
  backend: BackendTarget | null;
}

export function ExtensionWebviewPanel({
  tab,
  uri,
  tabId,
}: ExtensionWebviewPanelProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const workspaceData = useWorkspaceData();

  const availability = useMemo(
    () => resolveAvailability(workspaceData, tab),
    [workspaceData, tab],
  );

  const handleBridgeRequest = useCallback(
    async (message: ExtensionBridgeRequestMessage): Promise<JsonValue> => {
      const projectId = workspaceData.projectId;
      const sessionId = workspaceData.sessionId;
      if (!projectId || !sessionId) {
        throw new Error("Extension panel 缺少 Project 或 Session context");
      }
      if (!availability.backend) {
        throw new Error("Extension panel 缺少可用 backend");
      }

      switch (message.method) {
        case "metadata.get_context":
          return {
            project_id: projectId,
            extension_id: tab.extension_id,
            extension_key: tab.extension_key,
            panel_type_id: tab.type_id,
            uri,
          };
        case "workspace.open_tab": {
          const typeId = bridgeParamString(message.params, "type_id");
          const targetUri = bridgeParamString(message.params, "uri");
          if (!typeId || !targetUri) {
            throw new Error("workspace.open_tab 参数非法");
          }
          useWorkspaceTabStore.getState().openOrActivate(typeId, targetUri);
          return null;
        }
        case "runtime.invoke_action": {
          const actionKey = bridgeParamString(message.params, "action_key");
          if (!actionKey) {
            throw new Error("runtime.invoke_action 缺少 action_key");
          }
          const action = workspaceData.extensionRuntime.projection.runtime_actions.find(
            (item) => item.extension_key === tab.extension_key && item.action_key === actionKey,
          );
          if (!action) {
            throw new Error(`Extension action 不可用: ${actionKey}`);
          }
          const result = await invokeProjectExtensionRuntimeAction(projectId, {
            session_id: sessionId,
            backend_id: availability.backend.backend_id,
            action_key: actionKey,
            input: toJsonValue(message.params.input),
          });
          return result.output.output;
        }
        case "extension.invoke_channel": {
          const channelKey = bridgeParamString(message.params, "channel_key");
          const method = bridgeParamString(message.params, "method");
          if (!channelKey || !method) {
            throw new Error("extension.invoke_channel 参数非法");
          }
          const dependencyAlias = bridgeParamString(message.params, "dependency_alias");
          const result = await invokeProjectExtensionRuntimeChannel(projectId, {
            session_id: sessionId,
            backend_id: availability.backend.backend_id,
            channel_key: channelKey,
            method,
            input: toJsonValue(message.params.input),
            consumer_extension_key: tab.extension_key,
            dependency_alias: dependencyAlias || null,
          });
          return result.output.output;
        }
        case "vfs.read": {
          const path = bridgeParamString(message.params, "path");
          if (!path) {
            throw new Error("vfs.read 缺少 path");
          }
          const target = resolvePanelVfsTarget(workspaceData);
          const result = await readSurfaceFile({
            surfaceRef: target.surfaceRef,
            mountId: target.mountId,
            path,
          });
          return result.content;
        }
        case "vfs.write": {
          const path = bridgeParamString(message.params, "path");
          const content = bridgeParamRawString(message.params, "content");
          if (!path) {
            throw new Error("vfs.write 缺少 path");
          }
          const target = resolvePanelVfsTarget(workspaceData);
          await writeSurfaceFile({
            surfaceRef: target.surfaceRef,
            mountId: target.mountId,
            path,
            content,
          });
          return null;
        }
        default:
          throw new Error(`未知 Extension bridge method: ${message.method}`);
      }
    },
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

function bridgeParamRawString(
  params: Record<string, unknown>,
  key: string,
): string {
  const value = params[key];
  return typeof value === "string" ? value : "";
}

function resolvePanelVfsTarget(
  workspaceData: WorkspaceData,
): { surfaceRef: string; mountId: string } {
  const surface = workspaceData.runtimeSurface;
  if (!surface) {
    throw new Error("Extension VFS bridge 缺少 runtime surface");
  }
  const mountId = surface.default_mount_id
    ?? surface.mounts.find((mount) => mount.backend_id.trim() !== "")?.id
    ?? surface.mounts[0]?.id
    ?? "";
  if (!mountId) {
    throw new Error("Extension VFS bridge 缺少可用 mount");
  }
  return {
    surfaceRef: surface.surface_ref,
    mountId,
  };
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
  if (!workspaceData.projectId || !workspaceData.sessionId) {
    return unavailable("Extension panel 不可用", "当前页面缺少 Project 或 Session context。");
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
  const backend = selectBackendTarget(workspaceData);
  if (!backend) {
    return unavailable("Backend 不可用", "当前 Project workspace 没有可用 backend。");
  }
  if (!backend.online) {
    return {
      ...unavailable("Backend 离线", `${backend.label} 当前离线。`),
      backend,
    };
  }
  const entry = tab.renderer.entry.trim();
  if (!entry) {
    return unavailable("Extension bundle 缺失", "插件 panel renderer 缺少 entry。");
  }

  return {
    available: true,
    title: "",
    detail: "",
    src: buildExtensionWebviewAssetUrl(workspaceData.projectId, tab.extension_key, entry),
    backend,
  };
}

function unavailable(title: string, detail: string): Availability {
  return {
    available: false,
    title,
    detail,
    src: null,
    backend: null,
  };
}

function selectBackendTarget(
  workspaceData: WorkspaceData,
): BackendTarget | null {
  const runtimeBackend = selectRuntimeSurfaceBackend(workspaceData.runtimeSurface);
  return runtimeBackend ?? workspaceData.workspaceBackend;
}

function selectRuntimeSurfaceBackend(
  runtimeSurface: WorkspaceData["runtimeSurface"],
): BackendTarget | null {
  const mounts = runtimeSurface?.mounts ?? [];
  const defaultMount = runtimeSurface?.default_mount_id
    ? mounts.find((mount) => mount.id === runtimeSurface.default_mount_id) ?? null
    : null;
  const ordered = defaultMount
    ? [defaultMount, ...mounts.filter((mount) => mount.id !== defaultMount.id)]
    : mounts;
  const selected = ordered.find((mount) => mount.backend_id.trim() !== "");
  return selected ? backendTargetFromMount(selected) : null;
}

function backendTargetFromMount(mount: ResolvedMountSummary): BackendTarget {
  return {
    backend_id: mount.backend_id,
    label: mount.display_name || mount.backend_id,
    online: mount.backend_online !== false,
  };
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
