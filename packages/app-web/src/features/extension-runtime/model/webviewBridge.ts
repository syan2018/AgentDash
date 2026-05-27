import type { JsonValue } from "../../../generated/extension-runtime-contracts";
import type {
  ExtensionRuntimeInvokeActionRequest,
  ExtensionRuntimeInvokeActionResponse,
  ExtensionRuntimeInvokeChannelRequest,
  ExtensionRuntimeInvokeChannelResponse,
  ExtensionWorkspaceTabProjectionResponse,
} from "../../../generated/extension-runtime-contracts";
import type { ResolvedMountSummary } from "../../../types";
import { buildExtensionWebviewAssetUrl } from "../../../services/extensionRuntime";
import type { WorkspaceData } from "../../workspace-panel/workspace-data-context";
import type { WorkspaceBackendTarget } from "../../workspace-panel/workspace-panel-types";
import {
  bridgeParamString,
  toJsonValue,
  type ExtensionBridgeRequestMessage,
} from "./bridge";

type BackendTarget = WorkspaceBackendTarget;

export interface ExtensionWebviewAvailability {
  available: boolean;
  title: string;
  detail: string;
  src: string | null;
  backend: BackendTarget | null;
}

export interface ExtensionWebviewBridgeServices {
  openTab(typeId: string, uri: string): void;
  invokeAction(
    projectId: string,
    request: ExtensionRuntimeInvokeActionRequest,
  ): Promise<ExtensionRuntimeInvokeActionResponse>;
  invokeChannel(
    projectId: string,
    request: ExtensionRuntimeInvokeChannelRequest,
  ): Promise<ExtensionRuntimeInvokeChannelResponse>;
  readFile(request: { surfaceRef: string; mountId: string; path: string }): Promise<{ content: string }>;
  writeFile(
    request: { surfaceRef: string; mountId: string; path: string; content: string },
  ): Promise<unknown>;
}

export async function handleExtensionWebviewBridgeRequest({
  message,
  workspaceData,
  tab,
  uri,
  backend,
  services,
}: {
  message: ExtensionBridgeRequestMessage;
  workspaceData: WorkspaceData;
  tab: ExtensionWorkspaceTabProjectionResponse;
  uri: string;
  backend: BackendTarget | null;
  services: ExtensionWebviewBridgeServices;
}): Promise<JsonValue> {
  const projectId = workspaceData.projectId;
  const sessionId = workspaceData.sessionId;
  if (!projectId || !sessionId) {
    throw new Error("Extension panel 缺少 Project 或 Session context");
  }
  if (!backend) {
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
      services.openTab(typeId, targetUri);
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
      const result = await services.invokeAction(projectId, {
        session_id: sessionId,
        backend_id: backend.backend_id,
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
      const result = await services.invokeChannel(projectId, {
        session_id: sessionId,
        backend_id: backend.backend_id,
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
      const result = await services.readFile({
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
      await services.writeFile({
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
}

export function resolveExtensionWebviewAvailability(
  workspaceData: WorkspaceData,
  tab: ExtensionWorkspaceTabProjectionResponse,
): ExtensionWebviewAvailability {
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
  const backend = selectExtensionBackendTarget(workspaceData);
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

export function selectExtensionBackendTarget(
  workspaceData: WorkspaceData,
): BackendTarget | null {
  const runtimeBackend = selectRuntimeSurfaceBackend(workspaceData.runtimeSurface);
  return runtimeBackend ?? workspaceData.workspaceBackend;
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

function unavailable(title: string, detail: string): ExtensionWebviewAvailability {
  return {
    available: false,
    title,
    detail,
    src: null,
    backend: null,
  };
}
