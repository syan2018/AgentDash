import type {
  AgentDashAgentExposureInput,
  AgentDashAppDefinition,
  AgentDashAppInput,
  AgentDashArtifactProjection,
  AgentDashBackendServiceOptions,
  AgentDashCapabilityAccess,
  AgentDashCapabilityDispatch,
  AgentDashCapabilityKind,
  AgentDashCapabilityRecipe,
  AgentDashCustomProtocolMethodOptions,
  AgentDashCustomProtocolOptions,
  AgentDashDispatchProjection,
  AgentDashExposureListInput,
  AgentDashHttpProxyOptions,
  AgentDashLocalCommandOptions,
  AgentDashNormalizedAgentExposure,
  AgentDashNormalizedCapability,
  AgentDashOperationCatalogEntry,
  AgentDashOperationDispatchProjection,
  AgentDashOperationVisibility,
  AgentDashPermissionDeclaration,
  AgentDashPermissionSummary,
  AgentDashPermissionSummaryItem,
  AgentDashRuntimePermissionKey,
  AgentDashWorkspaceFilesOptions,
  JsonSchema,
} from "./types.js";

const ID_PATTERN = /^[a-z0-9_-]+(?:\.[a-z0-9_-]+)*$/;
const CAPABILITY_KEY_PATTERN = /^[A-Za-z][A-Za-z0-9_-]*$/;
const QUALIFIED_KEY_PATTERN = /^[a-z0-9_-]+(?:\.[a-z0-9_-]+)*$/;
const METHOD_NAME_PATTERN = /^[A-Za-z][A-Za-z0-9_]*$/;
const DEFAULT_SCHEMA: JsonSchema = true;

export function defineApp<TCapabilities extends Record<string, AgentDashCapabilityRecipe>>(
  input: AgentDashAppInput<TCapabilities>,
): AgentDashAppDefinition {
  return normalizeAppDefinition(input);
}

export function normalizeAppDefinition(input: AgentDashAppInput): AgentDashAppDefinition {
  const id = requireTrimmed(input.id, "app.id");
  const name = requireTrimmed(input.name, "app.name");
  const version = requireTrimmed(input.version, "app.version");
  if (!ID_PATTERN.test(id)) {
    throw new Error("app.id must use lowercase letters, digits, underscores, hyphens, and dot segments");
  }

  const panelEntry = requireTrimmed(input.panel.entry, "panel.entry");
  const panel = {
    entry: panelEntry,
    title: input.panel.title?.trim() || name,
    type_id: normalizeOptionalQualifiedKey(input.panel.type_id, `${id}.panel`, "panel.type_id"),
    uri_scheme: normalizeUriScheme(input.panel.uri_scheme, id),
  };

  const capabilities: AgentDashNormalizedCapability[] = [];
  const dispatches: AgentDashDispatchProjection[] = [];
  const artifacts: AgentDashArtifactProjection[] = [{ kind: "panel", entry: panel.entry }];
  const exposures: AgentDashNormalizedAgentExposure[] = [];
  const permissionItems: AgentDashPermissionSummaryItem[] = [];
  const declarations: AgentDashPermissionDeclaration[] = [];
  const runtimePermissions: AgentDashRuntimePermissionKey[] = [];
  const usedWireKeys = new Set<string>();
  const usedOperationKeys = new Set<string>();

  for (const [capabilityKey, recipe] of Object.entries(input.capabilities ?? {})) {
    validateCapabilityKey(capabilityKey);
    const wireKey = toWireKeySegment(capabilityKey);
    if (usedWireKeys.has(wireKey)) {
      throw new Error(`capability key '${capabilityKey}' conflicts after normalization as '${wireKey}'`);
    }
    usedWireKeys.add(wireKey);

    const normalized = normalizeCapability(id, capabilityKey, wireKey, recipe);
    capabilities.push(normalized.capability);
    dispatches.push(normalized.dispatch);
    artifacts.push(...normalized.artifacts);
    declarations.push(...normalized.declarations);
    runtimePermissions.push(...normalized.dispatch.runtime_permissions);
    permissionItems.push(...normalized.permission_items);

    for (const exposure of normalized.exposures) {
      if (usedOperationKeys.has(exposure.operation_key)) {
        throw new Error(`operation projection '${exposure.operation_key}' is duplicated`);
      }
      usedOperationKeys.add(exposure.operation_key);
      exposures.push(exposure);
    }
  }

  const permissionSummary = buildPermissionSummary(runtimePermissions, declarations, permissionItems);
  const operationCatalog = exposures.map(operationFromExposure);

  return {
    kind: "agentdash.app",
    id,
    name,
    version,
    description: input.description?.trim() ?? "",
    panel,
    capabilities,
    agent_exposures: exposures,
    dispatches,
    artifacts,
    operation_catalog: operationCatalog,
    permission_summary: permissionSummary,
  };
}

export function isAgentDashRuntimePermissionKey(
  permission: string,
): permission is AgentDashRuntimePermissionKey {
  return permission === "local.profile.read"
    || hasPermissionScope(permission, "http.fetch")
    || permission === "workspace.vfs.read"
    || permission === "workspace.vfs.write"
    || permission === "workspace.vfs.list"
    || permission === "workspace.vfs.search"
    || hasPermissionScope(permission, "env.read")
    || permission === "process.exec"
    || permission === "process.shell"
    || hasPermissionScope(permission, "process.env.set")
    || hasPermissionScope(permission, "runtime.invoke")
    || hasPermissionScope(permission, "extension.protocol.invoke");
}

interface NormalizedCapabilityBundle {
  capability: AgentDashNormalizedCapability;
  dispatch: AgentDashDispatchProjection;
  artifacts: readonly AgentDashArtifactProjection[];
  exposures: readonly AgentDashNormalizedAgentExposure[];
  declarations: readonly AgentDashPermissionDeclaration[];
  permission_items: readonly AgentDashPermissionSummaryItem[];
}

function normalizeCapability(
  appId: string,
  capabilityKey: string,
  wireKey: string,
  recipe: AgentDashCapabilityRecipe,
): NormalizedCapabilityBundle {
  switch (recipe.kind) {
    case "http_proxy":
      return normalizeHttpProxy(appId, capabilityKey, wireKey, recipe.options);
    case "local_command":
      return normalizeLocalCommand(appId, capabilityKey, wireKey, recipe.options);
    case "workspace_files":
      return normalizeWorkspaceFiles(appId, capabilityKey, wireKey, recipe.options);
    case "custom_protocol":
      return normalizeCustomProtocol(appId, capabilityKey, wireKey, recipe.options);
    case "backend_service":
      return normalizeBackendService(appId, capabilityKey, wireKey, recipe.options);
  }
}

function normalizeHttpProxy(
  appId: string,
  capabilityKey: string,
  wireKey: string,
  options: AgentDashHttpProxyOptions,
): NormalizedCapabilityBundle {
  const baseUrl = requireUrl(options.baseUrl, `${capabilityKey}.baseUrl`);
  const host = baseUrl.host;
  const access = options.access ?? "read";
  const actionKey = `${appId}.${wireKey}`;
  validateQualifiedKey(actionKey, `${capabilityKey}.action_key`);
  const runtimePermissions: AgentDashRuntimePermissionKey[] = ["http.fetch", `http.fetch:${host}`];
  const declaration: AgentDashPermissionDeclaration = { kind: "http", hosts: [host], access };
  const permissionItems = permissionItemsForRuntime(capabilityKey, "http_proxy", runtimePermissions, declaration);
  const dispatch: AgentDashCapabilityDispatch = {
    kind: "runtime_action",
    action_key: actionKey,
    host_api: "http.fetch",
    http: {
      base_url: normalizeHttpBaseUrl(baseUrl),
      access,
      headers: options.headers,
    },
  };
  return {
    capability: capabilitySummary(capabilityKey, wireKey, "http_proxy", options, permissionItems),
    dispatch: dispatchProjection(capabilityKey, "http_proxy", dispatch, runtimePermissions),
    artifacts: [],
    exposures: normalizeExposures({
      capability_key: capabilityKey,
      capability_kind: "http_proxy",
      recipe: "http_proxy",
      expose: options.expose,
      default_operation_key: actionKey,
      default_dispatch: { kind: "runtime_action", action_key: actionKey },
      permission_summary: permissionItems.map((item) => item.label),
    }),
    declarations: [declaration],
    permission_items: permissionItems,
  };
}

function normalizeLocalCommand(
  appId: string,
  capabilityKey: string,
  wireKey: string,
  options: AgentDashLocalCommandOptions,
): NormalizedCapabilityBundle {
  requireTrimmed(options.command, `${capabilityKey}.command`);
  const actionKey = `${appId}.${wireKey}`;
  validateQualifiedKey(actionKey, `${capabilityKey}.action_key`);
  const mode = options.shell === true ? "shell" : "exec";
  const runtimePermission: AgentDashRuntimePermissionKey = mode === "shell" ? "process.shell" : "process.exec";
  const declaration: AgentDashPermissionDeclaration = { kind: "process", access: "execute", mode };
  const permissionItems = permissionItemsForRuntime(
    capabilityKey,
    "local_command",
    [runtimePermission],
    declaration,
  );
  const dispatch: AgentDashCapabilityDispatch = {
    kind: "runtime_action",
    action_key: actionKey,
    host_api: runtimePermission,
    command: {
      command: requireTrimmed(options.command, `${capabilityKey}.command`),
      args: [...(options.args ?? [])],
      shell: options.shell === true,
      cwd: options.cwd,
      env: options.env,
      timeout_ms: options.timeout_ms,
    },
  };
  return {
    capability: capabilitySummary(capabilityKey, wireKey, "local_command", options, permissionItems),
    dispatch: dispatchProjection(capabilityKey, "local_command", dispatch, [runtimePermission]),
    artifacts: [],
    exposures: normalizeExposures({
      capability_key: capabilityKey,
      capability_kind: "local_command",
      recipe: "local_command",
      expose: options.expose,
      default_operation_key: actionKey,
      default_dispatch: { kind: "runtime_action", action_key: actionKey },
      permission_summary: permissionItems.map((item) => item.label),
    }),
    declarations: [declaration],
    permission_items: permissionItems,
  };
}

function normalizeWorkspaceFiles(
  appId: string,
  capabilityKey: string,
  wireKey: string,
  options: AgentDashWorkspaceFilesOptions,
): NormalizedCapabilityBundle {
  const access = options.access ?? "read";
  const actionKey = `${appId}.${wireKey}`;
  validateQualifiedKey(actionKey, `${capabilityKey}.action_key`);
  const runtimePermissions = workspacePermissions(access);
  const declaration: AgentDashPermissionDeclaration = { kind: "workspace", access };
  const permissionItems = permissionItemsForRuntime(
    capabilityKey,
    "workspace_files",
    runtimePermissions,
    declaration,
  );
  const dispatch: AgentDashCapabilityDispatch = {
    kind: "runtime_action",
    action_key: actionKey,
    host_api: "workspace.vfs",
    workspace: {
      access,
      roots: [...(options.roots ?? [])],
    },
  };
  return {
    capability: capabilitySummary(capabilityKey, wireKey, "workspace_files", options, permissionItems),
    dispatch: dispatchProjection(capabilityKey, "workspace_files", dispatch, runtimePermissions),
    artifacts: [],
    exposures: normalizeExposures({
      capability_key: capabilityKey,
      capability_kind: "workspace_files",
      recipe: "workspace_files",
      expose: options.expose,
      default_operation_key: actionKey,
      default_dispatch: { kind: "runtime_action", action_key: actionKey },
      permission_summary: permissionItems.map((item) => item.label),
    }),
    declarations: [declaration],
    permission_items: permissionItems,
  };
}

function normalizeCustomProtocol(
  appId: string,
  capabilityKey: string,
  wireKey: string,
  options: AgentDashCustomProtocolOptions,
): NormalizedCapabilityBundle {
  const protocolKey = normalizeOptionalQualifiedKey(options.protocol_key, `${appId}.${wireKey}`, `${capabilityKey}.protocol_key`);
  const methods = Object.entries(options.methods);
  if (methods.length === 0) {
    throw new Error(`${capabilityKey}.methods must contain at least one method`);
  }

  const methodNames: string[] = [];
  const runtimePermissions: AgentDashRuntimePermissionKey[] = [`extension.protocol.invoke:${protocolKey}`];
  const methodExposures: AgentDashNormalizedAgentExposure[] = [];
  const permissionItems: AgentDashPermissionSummaryItem[] = [
    {
      capability_key: capabilityKey,
      capability_kind: "custom_protocol",
      label: `extension.protocol.invoke:${protocolKey}`,
      runtime_permission: `extension.protocol.invoke:${protocolKey}`,
    },
  ];

  for (const [methodName, method] of methods) {
    validateMethodName(methodName, `${capabilityKey}.methods.${methodName}`);
    methodNames.push(methodName);
    const methodPermissions = normalizeRuntimePermissionList(
      method.permissions ?? [],
      `${capabilityKey}.methods.${methodName}.permissions`,
    );
    runtimePermissions.push(...methodPermissions);
    for (const permission of methodPermissions) {
      permissionItems.push({
        capability_key: capabilityKey,
        capability_kind: "custom_protocol",
        label: permission,
        runtime_permission: permission,
      });
    }
    methodExposures.push(...normalizeChannelMethodExposures({
      capability_key: capabilityKey,
      method_name: methodName,
      method,
      protocol_key: protocolKey,
      permission_summary: permissionItems.map((item) => item.label),
    }));
  }

  const declaration: AgentDashPermissionDeclaration = {
    kind: "extension_protocol",
    protocol_key: protocolKey,
    methods: methodNames,
  };
  const dispatch: AgentDashCapabilityDispatch = {
    kind: "protocol_method",
    protocol_key: protocolKey,
    version: options.version?.trim() || "1.0.0",
    description: options.description?.trim() || titleFromKey(capabilityKey),
    methods: methods.map(([methodName, method]) => ({
      name: methodName,
      description: requireTrimmed(method.description, `${capabilityKey}.methods.${methodName}.description`),
      input_schema: method.input_schema ?? DEFAULT_SCHEMA,
      output_schema: method.output_schema ?? DEFAULT_SCHEMA,
      permissions: normalizeRuntimePermissionList(
        method.permissions ?? [],
        `${capabilityKey}.methods.${methodName}.permissions`,
      ),
    })),
  };
  return {
    capability: capabilitySummary(capabilityKey, wireKey, "custom_protocol", options, permissionItems),
    dispatch: dispatchProjection(capabilityKey, "custom_protocol", dispatch, uniqueRuntimePermissions(runtimePermissions)),
    artifacts: [],
    exposures: methodExposures,
    declarations: [declaration],
    permission_items: permissionItems,
  };
}

function normalizeBackendService(
  appId: string,
  capabilityKey: string,
  wireKey: string,
  options: AgentDashBackendServiceOptions,
): NormalizedCapabilityBundle {
  const entry = requireTrimmed(options.entry, `${capabilityKey}.entry`);
  if (options.routes.length === 0) {
    throw new Error(`${capabilityKey}.routes must contain at least one route`);
  }
  const routes = options.routes.map((route) => requireRoutePattern(route, `${capabilityKey}.routes[]`));
  const serviceKey = `${appId}.${wireKey}`;
  validateQualifiedKey(serviceKey, `${capabilityKey}.service_key`);
  const declaration: AgentDashPermissionDeclaration = { kind: "backend_service", service_key: serviceKey, routes };
  const permissionItems: AgentDashPermissionSummaryItem[] = [{
    capability_key: capabilityKey,
    capability_kind: "backend_service",
    label: `backend_service:${serviceKey}`,
    declaration,
  }];
  const dispatch: AgentDashCapabilityDispatch = {
    kind: "backend_service",
    service_key: serviceKey,
    runtime: options.runtime ?? "node",
    entry,
    routes,
    health_path: options.healthPath,
  };
  return {
    capability: capabilitySummary(capabilityKey, wireKey, "backend_service", options, permissionItems),
    dispatch: dispatchProjection(capabilityKey, "backend_service", dispatch, []),
    artifacts: [{
      kind: "backend_service",
      capability_key: capabilityKey,
      service_key: serviceKey,
      runtime: options.runtime ?? "node",
      entry,
      routes,
      health_path: options.healthPath,
    }],
    exposures: normalizeExposures({
      capability_key: capabilityKey,
      capability_kind: "backend_service",
      recipe: "backend_service",
      expose: options.expose,
      default_operation_key: serviceKey,
      default_dispatch: { kind: "backend_service", service_key: serviceKey, route: routes[0] },
      permission_summary: permissionItems.map((item) => item.label),
    }),
    declarations: [declaration],
    permission_items: permissionItems,
  };
}

function capabilitySummary(
  capabilityKey: string,
  wireKey: string,
  kind: AgentDashCapabilityKind,
  options: { title?: string; description?: string },
  permissionItems: readonly AgentDashPermissionSummaryItem[],
): AgentDashNormalizedCapability {
  return {
    key: capabilityKey,
    wire_key: wireKey,
    kind,
    title: options.title?.trim() || titleFromKey(capabilityKey),
    description: options.description?.trim() ?? "",
    permission_summary: permissionItems.map((item) => item.label),
  };
}

function dispatchProjection(
  capabilityKey: string,
  capabilityKind: AgentDashCapabilityKind,
  dispatch: AgentDashCapabilityDispatch,
  runtimePermissions: readonly AgentDashRuntimePermissionKey[],
): AgentDashDispatchProjection {
  return {
    capability_key: capabilityKey,
    capability_kind: capabilityKind,
    dispatch,
    runtime_permissions: runtimePermissions,
  };
}

function normalizeExposures(input: {
  capability_key: string;
  capability_kind: AgentDashCapabilityKind;
  recipe: AgentDashCapabilityKind;
  expose: AgentDashExposureListInput | undefined;
  default_operation_key: string;
  default_dispatch: AgentDashOperationDispatchProjection;
  permission_summary: readonly string[];
}): AgentDashNormalizedAgentExposure[] {
  if (!input.expose) return [];
  return exposureList(input.expose).map((exposure) => {
    const exposureKey = normalizeExposureKey(exposure.key, input.capability_key);
    const operationKey = exposure.key ? exposureKey : input.default_operation_key;
    validateOperationVisibility(exposure.visibility ?? "agent_and_panel", `${input.capability_key}.expose.visibility`);
    return {
      capability_key: input.capability_key,
      exposure_key: exposureKey,
      operation_key: operationKey,
      visibility: exposure.visibility ?? "agent_and_panel",
      description: requireTrimmed(exposure.description, `${input.capability_key}.expose.description`),
      input_schema: exposure.input_schema ?? DEFAULT_SCHEMA,
      output_schema: exposure.output_schema ?? DEFAULT_SCHEMA,
      permission_summary: input.permission_summary,
      dispatch: input.default_dispatch,
      provenance: {
        source: "capability_exposure",
        capability_key: input.capability_key,
        exposure_key: exposureKey,
        capability_kind: input.capability_kind,
        recipe: input.recipe,
      },
    };
  });
}

function normalizeChannelMethodExposures(input: {
  capability_key: string;
  method_name: string;
  method: AgentDashCustomProtocolMethodOptions;
  protocol_key: string;
  permission_summary: readonly string[];
}): AgentDashNormalizedAgentExposure[] {
  if (!input.method.expose) return [];
  return exposureList(input.method.expose).map((exposure) => {
    const exposureKey = normalizeExposureKey(exposure.key, input.method_name);
    const operationKey = `${input.protocol_key}.${exposure.key ? exposureKey : input.method_name}`;
    validateOperationVisibility(exposure.visibility ?? "agent_and_panel", `${input.capability_key}.methods.${input.method_name}.expose.visibility`);
    return {
      capability_key: input.capability_key,
      exposure_key: exposureKey,
      operation_key: operationKey,
      visibility: exposure.visibility ?? "agent_and_panel",
      description: requireTrimmed(exposure.description, `${input.capability_key}.methods.${input.method_name}.expose.description`),
      input_schema: exposure.input_schema ?? input.method.input_schema ?? DEFAULT_SCHEMA,
      output_schema: exposure.output_schema ?? input.method.output_schema ?? DEFAULT_SCHEMA,
      permission_summary: input.permission_summary,
      dispatch: {
        kind: "protocol_method",
        protocol_key: input.protocol_key,
        method_name: input.method_name,
      },
      provenance: {
        source: "capability_exposure",
        capability_key: input.capability_key,
        exposure_key: exposureKey,
        capability_kind: "custom_protocol",
        recipe: "custom_protocol",
      },
    };
  });
}

function operationFromExposure(exposure: AgentDashNormalizedAgentExposure): AgentDashOperationCatalogEntry {
  return {
    operation_key: exposure.operation_key,
    visibility: exposure.visibility,
    origin: "capability_exposure",
    description: exposure.description,
    input_schema: exposure.input_schema,
    output_schema: exposure.output_schema,
    permission_summary: exposure.permission_summary,
    dispatch: exposure.dispatch,
    readiness: { kind: "ready" },
    provenance: exposure.provenance,
  };
}

function buildPermissionSummary(
  runtimePermissions: readonly AgentDashRuntimePermissionKey[],
  declarations: readonly AgentDashPermissionDeclaration[],
  items: readonly AgentDashPermissionSummaryItem[],
): AgentDashPermissionSummary {
  const byCapability = new Map<string, {
    capability_key: string;
    capability_kind: AgentDashCapabilityKind;
    permissions: AgentDashPermissionSummaryItem[];
  }>();
  for (const item of items) {
    const existing = byCapability.get(item.capability_key);
    if (existing) {
      existing.permissions.push(item);
    } else {
      byCapability.set(item.capability_key, {
        capability_key: item.capability_key,
        capability_kind: item.capability_kind,
        permissions: [item],
      });
    }
  }
  return {
    runtime_permissions: uniqueRuntimePermissions(runtimePermissions),
    declarations: uniqueDeclarations(declarations),
    items,
    by_capability: [...byCapability.values()],
  };
}

function permissionItemsForRuntime(
  capabilityKey: string,
  capabilityKind: AgentDashCapabilityKind,
  permissions: readonly AgentDashRuntimePermissionKey[],
  declaration: AgentDashPermissionDeclaration,
): AgentDashPermissionSummaryItem[] {
  return permissions.map((permission) => ({
    capability_key: capabilityKey,
    capability_kind: capabilityKind,
    label: permission,
    runtime_permission: permission,
    declaration,
  }));
}

function workspacePermissions(access: AgentDashCapabilityAccess): AgentDashRuntimePermissionKey[] {
  if (access === "read") {
    return ["workspace.vfs.read", "workspace.vfs.list", "workspace.vfs.search"];
  }
  if (access === "write") {
    return ["workspace.vfs.write"];
  }
  return ["workspace.vfs.read", "workspace.vfs.write", "workspace.vfs.list", "workspace.vfs.search"];
}

function exposureList(input: AgentDashExposureListInput): readonly AgentDashAgentExposureInput[] {
  return isExposureArray(input) ? input : [input];
}

function isExposureArray(
  input: AgentDashExposureListInput,
): input is readonly AgentDashAgentExposureInput[] {
  return Array.isArray(input);
}

function normalizeRuntimePermissionList(
  permissions: readonly string[],
  label: string,
): AgentDashRuntimePermissionKey[] {
  const normalized: AgentDashRuntimePermissionKey[] = [];
  for (const permission of permissions) {
    if (!isAgentDashRuntimePermissionKey(permission)) {
      throw new Error(`${label} contains unknown permission key: ${permission}`);
    }
    normalized.push(permission);
  }
  return normalized;
}

function uniqueRuntimePermissions(
  permissions: readonly AgentDashRuntimePermissionKey[],
): AgentDashRuntimePermissionKey[] {
  return [...new Set(permissions)].sort();
}

function uniqueDeclarations(
  declarations: readonly AgentDashPermissionDeclaration[],
): AgentDashPermissionDeclaration[] {
  const seen = new Set<string>();
  const result: AgentDashPermissionDeclaration[] = [];
  for (const declaration of declarations) {
    const key = JSON.stringify(declaration);
    if (!seen.has(key)) {
      seen.add(key);
      result.push(declaration);
    }
  }
  return result;
}

function requireTrimmed(value: string, label: string): string {
  const trimmed = value.trim();
  if (!trimmed) {
    throw new Error(`${label} must not be empty`);
  }
  return trimmed;
}

function requireUrl(value: string, label: string): URL {
  const trimmed = requireTrimmed(value, label);
  try {
    const parsed = new URL(trimmed);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      throw new Error("unsupported protocol");
    }
    return parsed;
  } catch (error) {
    throw new Error(`${label} must be an http or https URL`);
  }
}

function normalizeHttpBaseUrl(value: URL): string {
  value.hash = "";
  value.search = "";
  return value.href.replace(/\/$/, "");
}

function requireRoutePattern(value: string, label: string): string {
  const trimmed = requireTrimmed(value, label);
  if (!trimmed.startsWith("/")) {
    throw new Error(`${label} must start with '/'`);
  }
  return trimmed;
}

function validateCapabilityKey(value: string): void {
  if (!CAPABILITY_KEY_PATTERN.test(value)) {
    throw new Error(`capability key '${value}' must start with a letter and contain only letters, digits, underscores, or hyphens`);
  }
}

function validateQualifiedKey(value: string, label: string): void {
  if (!QUALIFIED_KEY_PATTERN.test(value)) {
    throw new Error(`${label} must use lowercase letters, digits, underscores, hyphens, and dot segments`);
  }
}

function validateMethodName(value: string, label: string): void {
  if (!METHOD_NAME_PATTERN.test(value)) {
    throw new Error(`${label} must be a valid method name`);
  }
}

function validateOperationVisibility(value: AgentDashOperationVisibility, label: string): void {
  if (value !== "panel_only" && value !== "agent_and_panel") {
    throw new Error(`${label} must be panel_only or agent_and_panel`);
  }
}

function normalizeExposureKey(value: string | undefined, fallback: string): string {
  if (!value) return fallback;
  validateQualifiedKey(value, "expose.key");
  return value;
}

function normalizeOptionalQualifiedKey(value: string | undefined, fallback: string, label: string): string {
  const normalized = value?.trim() || fallback;
  validateQualifiedKey(normalized, label);
  return normalized;
}

function normalizeUriScheme(value: string | undefined, appId: string): string {
  const normalized = value?.trim() || `agentdash-${appId.replaceAll(".", "-")}`;
  if (!/^[a-z][a-z0-9+.-]*$/.test(normalized)) {
    throw new Error("panel.uri_scheme must be a lowercase URI scheme");
  }
  return normalized;
}

function toWireKeySegment(value: string): string {
  return value
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .replaceAll("_", "-")
    .toLowerCase();
}

function titleFromKey(value: string): string {
  const words = toWireKeySegment(value).split("-").filter((word) => word.length > 0);
  return words.map((word) => `${word.slice(0, 1).toUpperCase()}${word.slice(1)}`).join(" ");
}

function hasPermissionScope(permission: string, base: string): boolean {
  return permission === base || permission.startsWith(`${base}:`);
}
