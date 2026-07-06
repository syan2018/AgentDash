// @ts-check

/**
 * @typedef {unknown} JsonValue
 * @typedef {Record<string, unknown>} JsonObject
 * @typedef {"session_runtime" | "setup"} ExtensionRuntimeActionKind
 * @typedef {JsonObject | boolean} JsonSchemaValue
 * @typedef {{ action_key: string, kind: ExtensionRuntimeActionKind, description: string, input_schema: JsonSchemaValue, output_schema: JsonSchemaValue, permissions?: string[], invoke(input: JsonValue): JsonValue | Promise<JsonValue> }} ExtensionRuntimeActionDefinition
 * @typedef {{ name: string, description: string, input_schema: JsonSchemaValue, output_schema: JsonSchemaValue, permissions?: string[], invoke(input: JsonValue): JsonValue | Promise<JsonValue> }} ExtensionProtocolChannelMethodDefinition
 * @typedef {{ name?: string, description: string, input_schema: JsonSchemaValue, output_schema: JsonSchemaValue, permissions?: string[], invoke(input: JsonValue): JsonValue | Promise<JsonValue> }} ExtensionProtocolChannelMethodMapEntry
 * @typedef {ExtensionProtocolChannelMethodDefinition[] | Record<string, ExtensionProtocolChannelMethodMapEntry>} ExtensionProtocolChannelMethodSet
 * @typedef {{ channel_key: string, version: string, description: string, methods: ExtensionProtocolChannelMethodSet }} ExtensionProtocolChannelRegistration
 * @typedef {{ channel_key: string, version: string, description: string, methods: ExtensionProtocolChannelMethodDefinition[] }} ExtensionProtocolChannelDefinition
 * @typedef {{ path: string, kind: "file" | "directory" }} ExtensionWorkspaceEntry
 * @typedef {{ path: string, kind: "file" | "directory" | "missing", size?: number, modified_at?: string }} ExtensionWorkspaceStat
 * @typedef {{ method?: string, headers?: Record<string, string>, body?: JsonValue | string, timeout_ms?: number }} ExtensionHttpRequestOptions
 * @typedef {{ status: number, headers: Record<string, string>, body: string }} ExtensionHttpResponse
 * @typedef {{ cwd?: string, env?: Record<string, string>, timeout_ms?: number, max_output_bytes?: number }} ExtensionProcessExecOptions
 * @typedef {{ exit_code: number, stdout: string, stderr: string, timed_out: boolean, truncated: boolean }} ExtensionProcessResult
 * @typedef {{ invoke<TInput extends JsonValue, TOutput extends JsonValue>(actionKey: string, input: TInput): Promise<TOutput> }} ExtensionRuntimeApi
 * @typedef {{ getProfile(): Promise<JsonObject> }} ExtensionLocalApi
 * @typedef {{ fetch(url: string, options?: ExtensionHttpRequestOptions): Promise<ExtensionHttpResponse>, fetchJson<TOutput extends JsonValue = JsonValue>(url: string, options?: ExtensionHttpRequestOptions): Promise<TOutput> }} ExtensionHttpApi
 * @typedef {{ readText(path: string): Promise<string>, writeText(path: string, content: string): Promise<void>, list(path: string): Promise<ExtensionWorkspaceEntry[]>, stat(path: string): Promise<ExtensionWorkspaceStat> }} ExtensionWorkspaceApi
 * @typedef {{ get(name: string): Promise<string | null> }} ExtensionEnvApi
 * @typedef {{ exec(command: string, args?: string[], options?: ExtensionProcessExecOptions): Promise<ExtensionProcessResult>, shell(command: string, options?: ExtensionProcessExecOptions): Promise<ExtensionProcessResult> }} ExtensionProcessApi
 * @typedef {{ invoke<TInput extends JsonValue, TOutput extends JsonValue>(method: string, input: TInput): Promise<TOutput> }} ExtensionChannelClient
 * @typedef {{ invoke<TInput extends JsonValue, TOutput extends JsonValue>(channelKey: string, method: string, input: TInput): Promise<TOutput>, self(channelKey?: string): ExtensionChannelClient, from(alias: string, channelKey?: string): ExtensionChannelClient }} ExtensionChannelsApi
 * @typedef {{ runtime: ExtensionRuntimeApi, local: ExtensionLocalApi, http: ExtensionHttpApi, workspace: ExtensionWorkspaceApi, env: ExtensionEnvApi, process: ExtensionProcessApi, channels: ExtensionChannelsApi }} ExtensionApi
 * @typedef {{ runtime?: Partial<ExtensionRuntimeApi>, local?: Partial<ExtensionLocalApi>, http?: Partial<ExtensionHttpApi>, workspace?: Partial<ExtensionWorkspaceApi>, env?: Partial<ExtensionEnvApi>, process?: Partial<ExtensionProcessApi>, channels?: Partial<ExtensionChannelsApi> }} ExtensionApiOverrides
 * @typedef {{ commands: unknown[], flags: unknown[], runtime_actions: ExtensionRuntimeActionDefinition[], protocol_channels: ExtensionProtocolChannelDefinition[], workspace_panels: unknown[], permissions: unknown[] }} ExtensionContributions
 * @typedef {{ api: ExtensionApi, commands: { registerCommand(definition: unknown): void }, flags: { registerFlag(definition: unknown): void }, runtime: { registerAction(definition: ExtensionRuntimeActionDefinition): void }, channels: { register(definition: ExtensionProtocolChannelRegistration): void }, workspace: { registerPanel(definition: unknown): void }, permissions: { require(permission: unknown): void }, contributions: ExtensionContributions }} ExtensionContext
 */

/**
 * @param {ExtensionApiOverrides} [api]
 * @returns {ExtensionContext}
 */
export function createExtensionContext(api = {}) {
  /** @type {ExtensionContributions} */
  const contributions = {
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

/**
 * @param {ExtensionProtocolChannelRegistration} definition
 * @returns {ExtensionProtocolChannelDefinition}
 */
function normalizeProtocolChannelDefinition(definition) {
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

/**
 * @param {ExtensionApiOverrides} overrides
 * @returns {ExtensionApi}
 */
function createApi(overrides) {
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

/**
 * @returns {ExtensionApi}
 */
function createNoopApi() {
  /** @param {string} method */
  const notConnected = (method) => {
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
