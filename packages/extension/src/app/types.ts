export type JsonPrimitive = string | number | boolean | null;
export type JsonObject = { readonly [key: string]: JsonValue };
export type JsonValue = JsonPrimitive | readonly JsonValue[] | JsonObject;
export type JsonSchema = JsonObject | boolean;

export type AgentDashCapabilityAccess = "read" | "write" | "read_write";
export type AgentDashRuntimePermissionKey =
  | "local.profile.read"
  | "http.fetch"
  | `http.fetch:${string}`
  | "workspace.vfs.read"
  | "workspace.vfs.write"
  | "workspace.vfs.list"
  | "workspace.vfs.search"
  | "env.read"
  | `env.read:${string}`
  | "process.exec"
  | "process.shell"
  | "process.env.set"
  | `process.env.set:${string}`
  | "runtime.invoke"
  | `runtime.invoke:${string}`
  | "extension.protocol.invoke"
  | `extension.protocol.invoke:${string}`;

export type AgentDashCapabilityKind =
  | "http_proxy"
  | "local_command"
  | "workspace_files"
  | "custom_protocol"
  | "backend_service";

export type AgentDashOperationVisibility = "panel_only" | "agent_and_panel";

export interface AgentDashPanelDefinitionInput {
  entry: string;
  title?: string;
  type_id?: string;
  uri_scheme?: string;
}

export interface AgentDashPanelDefinition {
  entry: string;
  title: string;
  type_id: string;
  uri_scheme: string;
}

export interface AgentDashAgentExposureInput {
  key?: string;
  description: string;
  input_schema?: JsonSchema;
  output_schema?: JsonSchema;
  visibility?: AgentDashOperationVisibility;
}

export interface AgentDashNormalizedAgentExposure {
  capability_key: string;
  exposure_key: string;
  operation_key: string;
  visibility: AgentDashOperationVisibility;
  description: string;
  input_schema: JsonSchema;
  output_schema: JsonSchema;
  permission_summary: readonly string[];
  dispatch: AgentDashOperationDispatchProjection;
  provenance: AgentDashOperationProvenance;
}

export interface AgentDashOperationProvenance {
  source: "capability_exposure";
  capability_key: string;
  exposure_key: string;
  capability_kind: AgentDashCapabilityKind;
  recipe: AgentDashCapabilityKind;
}

export type AgentDashOperationDispatchProjection =
  | { kind: "runtime_action"; action_key: string }
  | { kind: "protocol_method"; protocol_key: string; method_name: string }
  | { kind: "backend_service"; service_key: string; route?: string };

export interface AgentDashOperationCatalogEntry {
  operation_key: string;
  visibility: AgentDashOperationVisibility;
  origin: "capability_exposure";
  description: string;
  input_schema: JsonSchema;
  output_schema: JsonSchema;
  permission_summary: readonly string[];
  dispatch: AgentDashOperationDispatchProjection;
  readiness: { kind: "ready" };
  provenance: AgentDashOperationProvenance;
}

export interface AgentDashHttpProxyDispatchConfig {
  base_url: string;
  access: AgentDashCapabilityAccess;
  headers?: Record<string, string>;
}

export interface AgentDashLocalCommandDispatchConfig {
  command: string;
  args: readonly string[];
  shell: boolean;
  cwd?: string;
  env?: Record<string, string>;
  timeout_ms?: number;
}

export interface AgentDashWorkspaceFilesDispatchConfig {
  access: AgentDashCapabilityAccess;
  roots: readonly string[];
}

export interface AgentDashProtocolMethodDispatch {
  name: string;
  description: string;
  input_schema: JsonSchema;
  output_schema: JsonSchema;
  permissions: readonly AgentDashRuntimePermissionKey[];
}

export type AgentDashCapabilityDispatch =
  | {
      kind: "runtime_action";
      action_key: string;
      host_api: "http.fetch";
      http: AgentDashHttpProxyDispatchConfig;
    }
  | {
      kind: "runtime_action";
      action_key: string;
      host_api: "process.exec" | "process.shell";
      command: AgentDashLocalCommandDispatchConfig;
    }
  | {
      kind: "runtime_action";
      action_key: string;
      host_api: "workspace.vfs";
      workspace: AgentDashWorkspaceFilesDispatchConfig;
    }
  | {
      kind: "protocol_method";
      protocol_key: string;
      version: string;
      description: string;
      methods: readonly AgentDashProtocolMethodDispatch[];
    }
  | {
      kind: "backend_service";
      service_key: string;
      runtime: AgentDashBackendServiceRuntime;
      entry: string;
      routes: readonly string[];
      health_path?: string;
    };

export interface AgentDashDispatchProjection {
  capability_key: string;
  capability_kind: AgentDashCapabilityKind;
  dispatch: AgentDashCapabilityDispatch;
  runtime_permissions: readonly AgentDashRuntimePermissionKey[];
}

export type AgentDashArtifactProjection =
  | {
      kind: "panel";
      entry: string;
    }
  | {
      kind: "backend_service";
      capability_key: string;
      service_key: string;
      runtime: AgentDashBackendServiceRuntime;
      entry: string;
      routes: readonly string[];
      health_path?: string;
    };

export type AgentDashPermissionDeclaration =
  | { kind: "http"; hosts: readonly string[]; access: AgentDashCapabilityAccess }
  | { kind: "workspace"; access: AgentDashCapabilityAccess }
  | { kind: "process"; access: "execute"; mode: "exec" | "shell" }
  | { kind: "extension_protocol"; protocol_key: string; methods: readonly string[] }
  | { kind: "backend_service"; service_key: string; routes: readonly string[] };

export interface AgentDashPermissionSummaryItem {
  capability_key: string;
  capability_kind: AgentDashCapabilityKind;
  label: string;
  runtime_permission?: AgentDashRuntimePermissionKey;
  declaration?: AgentDashPermissionDeclaration;
}

export interface AgentDashCapabilityPermissionSummary {
  capability_key: string;
  capability_kind: AgentDashCapabilityKind;
  permissions: readonly AgentDashPermissionSummaryItem[];
}

export interface AgentDashPermissionSummary {
  runtime_permissions: readonly AgentDashRuntimePermissionKey[];
  declarations: readonly AgentDashPermissionDeclaration[];
  items: readonly AgentDashPermissionSummaryItem[];
  by_capability: readonly AgentDashCapabilityPermissionSummary[];
}

export interface AgentDashNormalizedCapability {
  key: string;
  wire_key: string;
  kind: AgentDashCapabilityKind;
  title: string;
  description: string;
  permission_summary: readonly string[];
}

export interface AgentDashAppInput<
  TCapabilities extends AgentDashCapabilityMapInput = AgentDashCapabilityMapInput,
> {
  id: string;
  name: string;
  version: string;
  description?: string;
  panel: AgentDashPanelDefinitionInput;
  capabilities?: TCapabilities;
}

export type AgentDashCapabilityMapInput = Record<string, AgentDashCapabilityRecipe>;

export interface AgentDashAppDefinition {
  kind: "agentdash.app";
  id: string;
  name: string;
  version: string;
  description: string;
  panel: AgentDashPanelDefinition;
  capabilities: readonly AgentDashNormalizedCapability[];
  agent_exposures: readonly AgentDashNormalizedAgentExposure[];
  dispatches: readonly AgentDashDispatchProjection[];
  artifacts: readonly AgentDashArtifactProjection[];
  operation_catalog: readonly AgentDashOperationCatalogEntry[];
  permission_summary: AgentDashPermissionSummary;
}

export interface AgentDashRecipeBaseOptions {
  title?: string;
  description?: string;
  expose?: AgentDashExposureListInput;
}

export type AgentDashExposureListInput =
  | AgentDashAgentExposureInput
  | readonly AgentDashAgentExposureInput[];

export interface AgentDashHttpProxyOptions extends AgentDashRecipeBaseOptions {
  baseUrl: string;
  access?: AgentDashCapabilityAccess;
  headers?: Record<string, string>;
}

export interface AgentDashLocalCommandOptions extends AgentDashRecipeBaseOptions {
  command: string;
  args?: readonly string[];
  shell?: boolean;
  cwd?: string;
  env?: Record<string, string>;
  timeout_ms?: number;
}

export interface AgentDashWorkspaceFilesOptions extends AgentDashRecipeBaseOptions {
  access?: AgentDashCapabilityAccess;
  roots?: readonly string[];
}

export interface AgentDashCustomProtocolMethodOptions {
  description: string;
  input_schema?: JsonSchema;
  output_schema?: JsonSchema;
  permissions?: readonly AgentDashRuntimePermissionKey[];
  expose?: AgentDashExposureListInput;
}

export interface AgentDashCustomProtocolOptions {
  title?: string;
  description?: string;
  protocol_key?: string;
  version?: string;
  methods: Record<string, AgentDashCustomProtocolMethodOptions>;
}

export type AgentDashBackendServiceRuntime = "node";

export interface AgentDashBackendServiceOptions extends AgentDashRecipeBaseOptions {
  entry: string;
  runtime?: AgentDashBackendServiceRuntime;
  routes: readonly string[];
  healthPath?: string;
}

export type AgentDashCapabilityRecipe =
  | { kind: "http_proxy"; options: AgentDashHttpProxyOptions }
  | { kind: "local_command"; options: AgentDashLocalCommandOptions }
  | { kind: "workspace_files"; options: AgentDashWorkspaceFilesOptions }
  | { kind: "custom_protocol"; options: AgentDashCustomProtocolOptions }
  | { kind: "backend_service"; options: AgentDashBackendServiceOptions };
