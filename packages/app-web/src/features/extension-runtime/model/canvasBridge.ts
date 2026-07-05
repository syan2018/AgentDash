import type { JsonValue } from "../../../generated/common-contracts";
import type {
  ExtensionRuntimeInvokeChannelRequest,
  ExtensionRuntimeInvokeChannelResponse,
  ExtensionWorkspaceTabProjectionResponse,
} from "../../../generated/extension-runtime-contracts";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import { buildExtensionWebviewAssetUrl } from "../../../services/extensionRuntime";
import type { CanvasExtensionChannelRequest } from "../../canvas-panel/CanvasRuntimePreview";
import type { WorkspaceData } from "../../workspace-runtime";
import { selectExtensionBackendTarget } from "./webviewBridge";

export interface ExtensionCanvasAvailability {
  available: boolean;
  title: string;
  detail: string;
  assetUrl: string | null;
}

export async function invokeExtensionChannelFromCanvas({
  workspaceData,
  tab,
  request,
  invokeChannel,
}: {
  workspaceData: WorkspaceData;
  tab: ExtensionWorkspaceTabProjectionResponse;
  request: CanvasExtensionChannelRequest;
  invokeChannel(
    target: AgentRunRuntimeTarget,
    request: ExtensionRuntimeInvokeChannelRequest,
  ): Promise<ExtensionRuntimeInvokeChannelResponse>;
}): Promise<unknown> {
  const agentRunTarget = workspaceData.agentRunRuntimeTarget ?? null;
  if (!workspaceData.projectId || !agentRunTarget) {
    throw new Error("Canvas extension channel 缺少 Project 或 AgentRun context");
  }
  const backend = selectExtensionBackendTarget(workspaceData);
  if (!backend || !backend.online) {
    throw new Error("Canvas extension channel 缺少可用 backend");
  }
  const result = await invokeChannel(agentRunTarget, {
    channel_key: request.channel_key,
    method: request.method,
    input: toJsonValue(request.input),
    consumer_extension_key: tab.extension_key,
    dependency_alias: request.dependency_alias ?? null,
  });
  return result.output.output;
}

export function resolveExtensionCanvasAvailability(
  workspaceData: WorkspaceData,
  tab: ExtensionWorkspaceTabProjectionResponse,
): ExtensionCanvasAvailability {
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
  if (!tab.loadability.available) {
    return unavailable(
      "Canvas extension 不可用",
      tab.loadability.reason ?? "当前插件 tab 不满足 renderer loadability 条件。",
    );
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

function toJsonValue(raw: unknown): JsonValue {
  if (raw === null || typeof raw === "string" || typeof raw === "boolean") return raw;
  if (typeof raw === "number") return Number.isFinite(raw) ? raw : null;
  if (Array.isArray(raw)) return raw.map(toJsonValue);
  if (raw == null || typeof raw !== "object") return null;
  const result: { [key: string]: JsonValue } = {};
  for (const [key, value] of Object.entries(raw)) {
    result[key] = toJsonValue(value);
  }
  return result;
}

function unavailable(title: string, detail: string): ExtensionCanvasAvailability {
  return {
    available: false,
    title,
    detail,
    assetUrl: null,
  };
}
