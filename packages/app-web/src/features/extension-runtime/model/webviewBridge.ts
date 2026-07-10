import type { JsonValue } from "../../../generated/common-contracts";
import type {
  ExtensionFetchRouteProjectionResponse,
  ExtensionGeneratedOperationDispatchResponse,
  ExtensionGeneratedOperationProjectionResponse,
  ExtensionWorkspaceTabProjectionResponse,
} from "../../../generated/extension-runtime-contracts";
import type {
  OperationWorkshopInvokeRequestDto,
  OperationWorkshopInvokeResponseDto,
} from "../../../generated/interaction-contracts";
import { buildExtensionWebviewAssetUrl } from "../../../services/extensionRuntime";
import type { WorkspaceBackendTarget, WorkspaceData } from "../../workspace-runtime";
import { selectDefaultVfsMount, selectVfsBackendTarget } from "../../vfs/vfs-browser-panel-policy";
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
  invokeOperation(
    projectId: string,
    request: OperationWorkshopInvokeRequestDto,
  ): Promise<OperationWorkshopInvokeResponseDto>;
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
  services,
}: {
  message: ExtensionBridgeRequestMessage;
  workspaceData: WorkspaceData;
  tab: ExtensionWorkspaceTabProjectionResponse;
  uri: string;
  services: ExtensionWebviewBridgeServices;
}): Promise<JsonValue> {
  const projectId = workspaceData.projectId;
  if (!projectId) {
    throw new Error("Extension panel 缺少 Project context");
  }
  const installation = workspaceData.extensionRuntime.projection.installations.find(
    (item) => item.extension_key === tab.extension_key,
  );
  if (!installation) throw new Error("Extension installation 不存在");

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
      const operation = findOperation(workspaceData, tab.extension_key, (dispatch) =>
        dispatch.kind === "runtime_action" && dispatch.action_key === actionKey);
      return invokeExtensionOperation(services, projectId, installation.installation_id, tab.extension_key, operation.operation_key, toJsonValue(message.params.input));
    }
    case "extension.invoke_protocol": {
      const protocolKey = bridgeParamString(message.params, "protocol_key");
      const method = bridgeParamString(message.params, "method");
      if (!protocolKey || !method) {
        throw new Error("extension.invoke_protocol 参数非法");
      }
      const operation = findOperation(workspaceData, tab.extension_key, (dispatch) =>
        dispatch.kind === "protocol_method"
        && dispatch.protocol_key === protocolKey
        && dispatch.method === method);
      return invokeExtensionOperation(services, projectId, installation.installation_id, tab.extension_key, operation.operation_key, toJsonValue(message.params.input));
    }
    case "fetch.request":
      return invokeBackendServiceFetchRoute({
        params: message.params,
        workspaceData,
        tab,
        installationId: installation.installation_id,
        services,
      });
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
  if (!workspaceData.projectId) {
    return unavailable("Extension panel 不可用", "当前页面缺少 Project context。");
  }
  if (!tab.loadability.available) {
    return unavailable(
      "Extension panel 不可用",
      tab.loadability.reason ?? "当前插件 tab 不满足 renderer loadability 条件。",
    );
  }
  const installation = workspaceData.extensionRuntime.projection.installations.find(
    (item) => item.extension_key === tab.extension_key,
  );
  if (!installation) {
    return unavailable("Extension 已停用", "当前 Project 没有启用这个插件。");
  }
  const backend = selectExtensionBackendTarget(workspaceData);
  const entry = tab.renderer.entry.trim();

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
  const runtimeBackend = workspaceData.runtimeSurface
    ? selectVfsBackendTarget(workspaceData.runtimeSurface.mounts, {
        defaultMountId: workspaceData.runtimeSurface.default_mount_id,
      })
    : null;
  return runtimeBackend ?? workspaceData.workspaceBackend;
}

function findOperation(
  workspaceData: WorkspaceData,
  extensionKey: string,
  predicate: (dispatch: ExtensionGeneratedOperationDispatchResponse) => boolean,
): ExtensionGeneratedOperationProjectionResponse {
  const operation = (workspaceData.extensionRuntime.projection.operation_catalog ?? []).find(
    (item) => item.extension_key === extensionKey && predicate(item.dispatch),
  );
  if (!operation) throw new Error("Extension capability 没有 canonical Operation exposure");
  return operation;
}

async function invokeExtensionOperation(
  services: ExtensionWebviewBridgeServices,
  projectId: string,
  installationId: string,
  extensionKey: string,
  operationKey: string,
  input: JsonValue,
): Promise<JsonValue> {
  const response = await services.invokeOperation(projectId, {
    context: { kind: "extension_panel", installation_id: installationId },
    operation_ref: {
      namespace: "extension",
      provider_key: extensionKey,
      operation_key: operationKey,
      contract_version: 1,
    },
    input,
    idempotency_key: null,
  });
  const result = asRecord(response.result);
  const value = asRecord(result?.value);
  if (value?.kind !== "inline") {
    throw new Error("Extension Operation 返回了需要 host resolve 的 result ref");
  }
  return toJsonValue(value.value);
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function bridgeParamRawString(
  params: Record<string, unknown>,
  key: string,
): string {
  const value = params[key];
  return typeof value === "string" ? value : "";
}

async function invokeBackendServiceFetchRoute({
  params,
  workspaceData,
  tab,
  installationId,
  services,
}: {
  params: Record<string, unknown>;
  workspaceData: WorkspaceData;
  tab: ExtensionWorkspaceTabProjectionResponse;
  installationId: string;
  services: ExtensionWebviewBridgeServices;
}): Promise<JsonValue> {
  const url = bridgeParamString(params, "url");
  if (!url) {
    throw new Error("fetch.request 缺少 url");
  }
  const routePath = pathWithSearchFromUrl(url);
  const matchedRoute = matchBackendServiceFetchRoute(
    workspaceData.extensionRuntime.projection.fetch_routes ?? [],
    tab.extension_key,
    routePath,
    url,
  );
  if (!matchedRoute || matchedRoute.target.kind !== "backend_service") {
    throw new Error(`Extension fetch route 未匹配: ${routePath}`);
  }
  const requestedServiceKey = backendServiceKeyFromBridgeRoute(params.route);
  if (requestedServiceKey && requestedServiceKey !== matchedRoute.target.service_key) {
    throw new Error(`Extension fetch route target mismatch: ${routePath}`);
  }

  const body = params.body;
  const routeTarget = matchedRoute.target;
  const operation = findOperation(workspaceData, tab.extension_key, (dispatch) =>
    dispatch.kind === "backend_service"
    && dispatch.service_key === routeTarget.service_key
    && dispatch.route === routeTarget.route);
  const output = await invokeExtensionOperation(services, workspaceData.projectId!, installationId, tab.extension_key, operation.operation_key, {
    method: bridgeParamString(params, "method") || "GET",
    headers: stringRecord(params.headers),
    body: typeof body === "string" ? body : null,
  });
  const result = asRecord(output);
  const response = asRecord(result?.response);
  const diagnostic = asRecord(result?.diagnostic);
  if (diagnostic) throw new Error(`Extension backendService 不可用: ${String(diagnostic.message ?? "unknown")}`);
  if (!response) {
    throw new Error("Extension backendService 缺少 HTTP response");
  }
  return {
    status: Number(response.status),
    headers: stringRecord(response.headers),
    body: noBodyStatus(Number(response.status))
      ? null
      : bytesToString(Array.isArray(response.body) ? response.body as number[] : null),
  };
}

function matchBackendServiceFetchRoute(
  routes: ExtensionFetchRouteProjectionResponse[],
  extensionKey: string,
  routePath: string,
  requestUrl: string,
): ExtensionFetchRouteProjectionResponse | null {
  for (const route of routes) {
    if (route.extension_key !== extensionKey) continue;
    if (route.target.kind !== "backend_service") continue;
    if (!routePatternMatches(route.pattern, routePath, requestUrl)) continue;
    if (!routePatternMatches(route.target.route, routePath, requestUrl)) continue;
    return route;
  }
  return null;
}

function pathWithSearchFromUrl(value: string): string {
  const parsed = new URL(value, "https://agentdash.local/");
  return `${parsed.pathname}${parsed.search}`;
}

function routePatternMatches(pattern: string, candidatePath: string, requestUrl: string): boolean {
  const normalizedPattern = comparableRoutePattern(pattern);
  const normalizedCandidate = comparableRouteCandidate(normalizedPattern, candidatePath, requestUrl);
  if (normalizedPattern === normalizedCandidate) return true;
  if (normalizedPattern.endsWith("/**")) {
    const prefix = normalizedPattern.slice(0, -3);
    return normalizedCandidate === prefix || normalizedCandidate.startsWith(`${prefix}/`);
  }
  if (normalizedPattern.endsWith("*")) {
    return normalizedCandidate.startsWith(normalizedPattern.slice(0, -1));
  }
  return false;
}

function comparableRoutePattern(pattern: string): string {
  const normalized = stripQuery(pattern.trim());
  if (!isAbsoluteHttpUrl(normalized)) return normalized;
  const parsed = new URL(normalized);
  return `${parsed.origin}${parsed.pathname}`;
}

function comparableRouteCandidate(pattern: string, candidatePath: string, requestUrl: string): string {
  if (!isAbsoluteHttpUrl(pattern)) return stripQuery(candidatePath.trim());
  const parsed = new URL(requestUrl, "https://agentdash.local/");
  return `${parsed.origin}${parsed.pathname}`;
}

function isAbsoluteHttpUrl(value: string): boolean {
  return /^https?:\/\//i.test(value);
}

function stripQuery(value: string): string {
  const index = value.indexOf("?");
  return index < 0 ? value : value.slice(0, index);
}

function backendServiceKeyFromBridgeRoute(value: unknown): string | null {
  const route = value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
  const target = route?.target && typeof route.target === "object" && !Array.isArray(route.target)
    ? route.target as Record<string, unknown>
    : null;
  if (target?.kind !== "backend_service") return null;
  return typeof target.service_key === "string" && target.service_key.trim() !== ""
    ? target.service_key.trim()
    : null;
}

function stringRecord(value: unknown): Record<string, string> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  const result: Record<string, string> = {};
  for (const [key, item] of Object.entries(value)) {
    if (typeof item === "string") {
      result[key] = item;
    }
  }
  return result;
}

function bytesToString(value: number[] | null): string {
  if (!value || value.length === 0) return "";
  return new TextDecoder().decode(new Uint8Array(value));
}

function noBodyStatus(status: number): boolean {
  return status === 204 || status === 205 || status === 304;
}

function resolvePanelVfsTarget(
  workspaceData: WorkspaceData,
): { surfaceRef: string; mountId: string } {
  const surface = workspaceData.runtimeSurface;
  if (!surface) {
    throw new Error("Extension VFS bridge 缺少 runtime surface");
  }
  const mountId = selectDefaultVfsMount(surface.mounts, {
    defaultMountId: surface.default_mount_id,
  })?.id ?? "";
  if (!mountId) {
    throw new Error("Extension VFS bridge 缺少可用 mount");
  }
  return {
    surfaceRef: surface.surface_ref,
    mountId,
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
