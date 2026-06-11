export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export type ExtensionBundleKind = "extension_host";
export type ExtensionRuntimeActionKind = "session_runtime" | "setup";
export type ExtensionPermissionAccess = "read" | "write" | "read_write";
export type ExtensionProcessPermissionAccess = "execute";

export interface ExtensionCommandHandler {
  kind: "inject_message";
  content: string;
}

export interface ExtensionCommandDefinition {
  name: string;
  description: string;
  handler: ExtensionCommandHandler;
}

export type ExtensionFlagType = "bool" | "string";

export interface ExtensionFlagDefinition {
  name: string;
  type: ExtensionFlagType;
  default: boolean | string;
  description: string;
}

export interface ExtensionRuntimeActionDefinition<Input extends JsonValue = JsonValue, Output extends JsonValue = JsonValue> {
  action_key: string;
  kind: ExtensionRuntimeActionKind;
  description: string;
  input_schema: JsonObject | boolean;
  output_schema: JsonObject | boolean;
  permissions?: string[];
  invoke(input: Input): Output | Promise<Output>;
}

export interface ExtensionWorkspacePanelDefinition {
  type_id: string;
  label: string;
  uri_scheme: string;
  entry: string;
}

export type ExtensionPermissionDeclaration =
  | { kind: "local_profile"; access: ExtensionPermissionAccess }
  | { kind: "http"; hosts: string[]; access: ExtensionPermissionAccess }
  | { kind: "workspace"; access: ExtensionPermissionAccess }
  | { kind: "env"; names: string[]; access: ExtensionPermissionAccess }
  | { kind: "process"; access: ExtensionProcessPermissionAccess }
  | { kind: "runtime_action"; action_key: string }
  | { kind: "extension_channel"; channel_key: string; methods: string[] };

export interface ExtensionProtocolChannelMethodDefinition<
  Input extends JsonValue = JsonValue,
  Output extends JsonValue = JsonValue,
> {
  name: string;
  description: string;
  input_schema: JsonObject | boolean;
  output_schema: JsonObject | boolean;
  permissions?: string[];
  invoke(input: Input): Output | Promise<Output>;
}

export type ExtensionProtocolChannelMethodMapEntry<
  Input extends JsonValue = JsonValue,
  Output extends JsonValue = JsonValue,
> = Omit<ExtensionProtocolChannelMethodDefinition<Input, Output>, "name"> & {
  name?: string;
};

export type ExtensionProtocolChannelMethodSet<
  Input extends JsonValue = JsonValue,
  Output extends JsonValue = JsonValue,
> =
  | ExtensionProtocolChannelMethodDefinition<Input, Output>[]
  | Record<string, ExtensionProtocolChannelMethodMapEntry<Input, Output>>;

export interface ExtensionProtocolChannelRegistration<
  Input extends JsonValue = JsonValue,
  Output extends JsonValue = JsonValue,
> {
  channel_key: string;
  version: string;
  description: string;
  methods: ExtensionProtocolChannelMethodSet<Input, Output>;
}

export interface ExtensionProtocolChannelDefinition {
  channel_key: string;
  version: string;
  description: string;
  methods: ExtensionProtocolChannelMethodDefinition[];
}

export interface ExtensionProtocolChannelMethodManifestDefinition {
  name: string;
  description: string;
  input_schema: JsonObject | boolean;
  output_schema: JsonObject | boolean;
  permissions?: string[];
}

export interface ExtensionProtocolChannelManifestDefinition {
  channel_key: string;
  version: string;
  description: string;
  methods: ExtensionProtocolChannelMethodManifestDefinition[];
}

export interface ExtensionDependencyDeclaration {
  alias: string;
  extension_id: string;
  version: string;
  channels: string[];
}

export interface ExtensionBundleRef {
  kind: ExtensionBundleKind;
  entry: string;
  digest: string;
}

export interface ExtensionManifest {
  manifest_version: string;
  extension_id: string;
  package: {
    name: string;
    version: string;
  };
  asset_version: string;
  commands?: ExtensionCommandDefinition[];
  flags?: ExtensionFlagDefinition[];
  message_renderers?: Array<{ custom_type: string; renderer: { kind: "json_card" | "markdown" } }>;
  capability_directives?: Array<{ add: string } | { remove: string }>;
  asset_refs?: Array<{ asset_type: string; key: string; required: boolean }>;
  runtime_actions?: Array<Omit<ExtensionRuntimeActionDefinition, "invoke">>;
  protocol_channels?: ExtensionProtocolChannelManifestDefinition[];
  extension_dependencies?: ExtensionDependencyDeclaration[];
  workspace_tabs?: Array<{
    type_id: string;
    label: string;
    uri_scheme: string;
    renderer:
      | { kind: "webview"; entry: string }
      | { kind: "canvas_panel"; entry: string };
  }>;
  permissions?: ExtensionPermissionDeclaration[];
  bundles?: ExtensionBundleRef[];
}

export interface ExtensionContributions {
  commands: ExtensionCommandDefinition[];
  flags: ExtensionFlagDefinition[];
  runtime_actions: ExtensionRuntimeActionDefinition[];
  protocol_channels: ExtensionProtocolChannelDefinition[];
  workspace_panels: ExtensionWorkspacePanelDefinition[];
  permissions: ExtensionPermissionDeclaration[];
}

export interface ExtensionRuntimeApi {
  invoke<TInput extends JsonValue, TOutput extends JsonValue>(actionKey: string, input: TInput): Promise<TOutput>;
}

export interface ExtensionLocalApi {
  getProfile(): Promise<JsonObject>;
}

export interface ExtensionHttpRequestOptions {
  method?: string;
  headers?: Record<string, string>;
  body?: JsonValue | string;
  timeout_ms?: number;
}

export interface ExtensionHttpResponse {
  status: number;
  headers: Record<string, string>;
  body: string;
}

export interface ExtensionHttpApi {
  fetch(url: string, options?: ExtensionHttpRequestOptions): Promise<ExtensionHttpResponse>;
  fetchJson<TOutput extends JsonValue = JsonValue>(
    url: string,
    options?: ExtensionHttpRequestOptions,
  ): Promise<TOutput>;
}

export interface ExtensionWorkspaceEntry {
  path: string;
  kind: "file" | "directory";
}

export interface ExtensionWorkspaceStat {
  path: string;
  kind: "file" | "directory" | "missing";
  size?: number;
  modified_at?: string;
}

export interface ExtensionWorkspaceApi {
  readText(path: string): Promise<string>;
  writeText(path: string, content: string): Promise<void>;
  list(path: string): Promise<ExtensionWorkspaceEntry[]>;
  stat(path: string): Promise<ExtensionWorkspaceStat>;
}

export interface ExtensionEnvApi {
  get(name: string): Promise<string | null>;
}

export interface ExtensionProcessExecOptions {
  cwd?: string;
  env?: Record<string, string>;
  timeout_ms?: number;
  max_output_bytes?: number;
}

export interface ExtensionProcessResult {
  exit_code: number;
  stdout: string;
  stderr: string;
  timed_out: boolean;
  truncated: boolean;
}

export interface ExtensionProcessApi {
  exec(command: string, args?: string[], options?: ExtensionProcessExecOptions): Promise<ExtensionProcessResult>;
  shell(command: string, options?: ExtensionProcessExecOptions): Promise<ExtensionProcessResult>;
}

export interface ExtensionChannelClient {
  invoke<TInput extends JsonValue, TOutput extends JsonValue>(method: string, input: TInput): Promise<TOutput>;
}

export interface ExtensionChannelsApi {
  invoke<TInput extends JsonValue, TOutput extends JsonValue>(
    channelKey: string,
    method: string,
    input: TInput,
  ): Promise<TOutput>;
  self(channelKey?: string): ExtensionChannelClient;
  from(alias: string, channelKey?: string): ExtensionChannelClient;
}

export interface ExtensionApi {
  runtime: ExtensionRuntimeApi;
  local: ExtensionLocalApi;
  http: ExtensionHttpApi;
  workspace: ExtensionWorkspaceApi;
  env: ExtensionEnvApi;
  process: ExtensionProcessApi;
  channels: ExtensionChannelsApi;
}

export interface ExtensionApiOverrides {
  runtime?: Partial<ExtensionRuntimeApi>;
  local?: Partial<ExtensionLocalApi>;
  http?: Partial<ExtensionHttpApi>;
  workspace?: Partial<ExtensionWorkspaceApi>;
  env?: Partial<ExtensionEnvApi>;
  process?: Partial<ExtensionProcessApi>;
  channels?: Partial<ExtensionChannelsApi>;
}

export interface ExtensionContext {
  api: ExtensionApi;
  commands: {
    registerCommand(definition: ExtensionCommandDefinition): void;
  };
  flags: {
    registerFlag(definition: ExtensionFlagDefinition): void;
  };
  runtime: {
    registerAction<Input extends JsonValue, Output extends JsonValue>(
      definition: ExtensionRuntimeActionDefinition<Input, Output>,
    ): void;
  };
  channels: {
    register<Input extends JsonValue, Output extends JsonValue>(
      definition: ExtensionProtocolChannelRegistration<Input, Output>,
    ): void;
  };
  workspace: {
    registerPanel(definition: ExtensionWorkspacePanelDefinition): void;
  };
  permissions: {
    require(permission: ExtensionPermissionDeclaration): void;
  };
  contributions: ExtensionContributions;
}

export interface AgentDashExtension {
  manifest: ExtensionManifest;
  activate?(ctx: ExtensionContext): void | Promise<void>;
}

export function defineExtension(extension: AgentDashExtension): AgentDashExtension {
  return extension;
}

export function createExtensionContext(api: ExtensionApiOverrides = {}): ExtensionContext {
  const contributions: ExtensionContributions = {
    commands: [],
    flags: [],
    runtime_actions: [],
    protocol_channels: [],
    workspace_panels: [],
    permissions: [],
  };
  return {
    api: createApi(api),
    contributions,
    commands: {
      registerCommand(definition) {
        contributions.commands.push(definition);
      },
    },
    flags: {
      registerFlag(definition) {
        contributions.flags.push(definition);
      },
    },
    runtime: {
      registerAction(definition) {
        contributions.runtime_actions.push(definition);
      },
    },
    channels: {
      register(definition) {
        contributions.protocol_channels.push(normalizeProtocolChannelDefinition(definition));
      },
    },
    workspace: {
      registerPanel(definition) {
        contributions.workspace_panels.push(definition);
      },
    },
    permissions: {
      require(permission) {
        contributions.permissions.push(permission);
      },
    },
  };
}

function normalizeProtocolChannelDefinition(
  definition: ExtensionProtocolChannelRegistration,
): ExtensionProtocolChannelDefinition {
  const methods = Array.isArray(definition.methods)
    ? definition.methods
    : Object.entries(definition.methods).map(([name, method]) => ({
        ...method,
        name: method.name ?? name,
      }));
  return {
    channel_key: definition.channel_key,
    version: definition.version,
    description: definition.description,
    methods,
  };
}

function createApi(overrides: ExtensionApiOverrides): ExtensionApi {
  const base = createNoopApi();
  return {
    runtime: { ...base.runtime, ...overrides.runtime },
    local: { ...base.local, ...overrides.local },
    http: { ...base.http, ...overrides.http },
    workspace: { ...base.workspace, ...overrides.workspace },
    env: { ...base.env, ...overrides.env },
    process: { ...base.process, ...overrides.process },
    channels: { ...base.channels, ...overrides.channels },
  };
}

function createNoopApi(): ExtensionApi {
  const notConnected = (method: string): never => {
    throw new Error(`AgentDash extension host API is not connected: ${method}`);
  };
  return {
    runtime: {
      async invoke() {
        return notConnected("runtime.invoke");
      },
    },
    local: {
      async getProfile() {
        return notConnected("local.getProfile");
      },
    },
    http: {
      async fetch() {
        return notConnected("http.fetch");
      },
      async fetchJson() {
        return notConnected("http.fetchJson");
      },
    },
    workspace: {
      async readText() {
        return notConnected("workspace.readText");
      },
      async writeText() {
        return notConnected("workspace.writeText");
      },
      async list() {
        return notConnected("workspace.list");
      },
      async stat() {
        return notConnected("workspace.stat");
      },
    },
    env: {
      async get() {
        return notConnected("env.get");
      },
    },
    process: {
      async exec() {
        return notConnected("process.exec");
      },
      async shell() {
        return notConnected("process.shell");
      },
    },
    channels: {
      async invoke() {
        return notConnected("channels.invoke");
      },
      self() {
        return {
          async invoke() {
            return notConnected("channels.self.invoke");
          },
        };
      },
      from() {
        return {
          async invoke() {
            return notConnected("channels.from.invoke");
          },
        };
      },
    },
  };
}
