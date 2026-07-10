export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export type ExtensionBundleKind = "extension_host";
export type ExtensionRuntimeActionKind = "session_runtime" | "setup";
export type ExtensionPermissionAccess = "read" | "write" | "read_write";
export type ExtensionProcessPermissionAccess = "execute";
export type ExtensionRuntimePermissionKey =
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
  permissions?: ExtensionRuntimePermissionKey[];
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
  | { kind: "extension_protocol"; protocol_key: string; methods: string[] };

export interface ExtensionProtocolMethodDefinition<
  Input extends JsonValue = JsonValue,
  Output extends JsonValue = JsonValue,
> {
  name: string;
  description: string;
  input_schema: JsonObject | boolean;
  output_schema: JsonObject | boolean;
  permissions?: ExtensionRuntimePermissionKey[];
  invoke(input: Input): Output | Promise<Output>;
}

export type ExtensionProtocolMethodMapEntry<
  Input extends JsonValue = JsonValue,
  Output extends JsonValue = JsonValue,
> = Omit<ExtensionProtocolMethodDefinition<Input, Output>, "name"> & {
  name?: string;
};

export type ExtensionProtocolMethodSet<
  Input extends JsonValue = JsonValue,
  Output extends JsonValue = JsonValue,
> =
  | ExtensionProtocolMethodDefinition<Input, Output>[]
  | Record<string, ExtensionProtocolMethodMapEntry<Input, Output>>;

export interface ExtensionProtocolRegistration<
  Input extends JsonValue = JsonValue,
  Output extends JsonValue = JsonValue,
> {
  protocol_key: string;
  version: string;
  description: string;
  methods: ExtensionProtocolMethodSet<Input, Output>;
}

export interface ExtensionProtocolDefinition {
  protocol_key: string;
  version: string;
  description: string;
  methods: ExtensionProtocolMethodDefinition[];
}

export interface ExtensionProtocolMethodManifestDefinition {
  name: string;
  description: string;
  input_schema: JsonObject | boolean;
  output_schema: JsonObject | boolean;
  permissions?: ExtensionRuntimePermissionKey[];
}

export interface ExtensionProtocolManifestDefinition {
  protocol_key: string;
  version: string;
  description: string;
  methods: ExtensionProtocolMethodManifestDefinition[];
}

export interface ExtensionDependencyDeclaration {
  alias: string;
  extension_id: string;
  version: string;
  protocols: string[];
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
  protocols?: ExtensionProtocolManifestDefinition[];
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
  protocols: ExtensionProtocolDefinition[];
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

export interface ExtensionProtocolClient {
  invoke<TInput extends JsonValue, TOutput extends JsonValue>(method: string, input: TInput): Promise<TOutput>;
}

export interface ExtensionProtocolsApi {
  invoke<TInput extends JsonValue, TOutput extends JsonValue>(
    protocolKey: string,
    method: string,
    input: TInput,
  ): Promise<TOutput>;
  self(protocolKey?: string): ExtensionProtocolClient;
  from(alias: string, protocolKey?: string): ExtensionProtocolClient;
}

export interface ExtensionApi {
  runtime: ExtensionRuntimeApi;
  local: ExtensionLocalApi;
  http: ExtensionHttpApi;
  workspace: ExtensionWorkspaceApi;
  env: ExtensionEnvApi;
  process: ExtensionProcessApi;
  protocols: ExtensionProtocolsApi;
}

export interface ExtensionApiOverrides {
  runtime?: Partial<ExtensionRuntimeApi>;
  local?: Partial<ExtensionLocalApi>;
  http?: Partial<ExtensionHttpApi>;
  workspace?: Partial<ExtensionWorkspaceApi>;
  env?: Partial<ExtensionEnvApi>;
  process?: Partial<ExtensionProcessApi>;
  protocols?: Partial<ExtensionProtocolsApi>;
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
  protocols: {
    register<Input extends JsonValue, Output extends JsonValue>(
      definition: ExtensionProtocolRegistration<Input, Output>,
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
    protocols: [],
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
    protocols: {
      register(definition) {
        contributions.protocols.push(normalizeProtocolMethodDefinition(definition));
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

function normalizeProtocolMethodDefinition(
  definition: ExtensionProtocolRegistration,
): ExtensionProtocolDefinition {
  const methods = Array.isArray(definition.methods)
    ? definition.methods
    : Object.entries(definition.methods).map(([name, method]) => ({
        ...method,
        name: method.name ?? name,
      }));
  return {
    protocol_key: definition.protocol_key,
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
    protocols: { ...base.protocols, ...overrides.protocols },
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
    protocols: {
      async invoke() {
        return notConnected("protocols.invoke");
      },
      self() {
        return {
          async invoke() {
            return notConnected("protocols.self.invoke");
          },
        };
      },
      from() {
        return {
          async invoke() {
            return notConnected("protocols.from.invoke");
          },
        };
      },
    },
  };
}
