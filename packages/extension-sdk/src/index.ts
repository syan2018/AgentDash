export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export type ExtensionBundleKind = "extension_host";
export type ExtensionRuntimeActionKind = "session_runtime" | "setup";
export type ExtensionPermissionAccess = "read" | "write" | "read_write";

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
  input_schema?: JsonObject | boolean;
  output_schema?: JsonObject | boolean;
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
  | { kind: "workspace"; access: ExtensionPermissionAccess }
  | { kind: "runtime_action"; action_key: string };

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
  workspace_tabs?: Array<{
    type_id: string;
    label: string;
    uri_scheme: string;
    renderer: { kind: "webview"; entry: string };
  }>;
  permissions?: ExtensionPermissionDeclaration[];
  bundles?: ExtensionBundleRef[];
}

export interface ExtensionContributions {
  commands: ExtensionCommandDefinition[];
  flags: ExtensionFlagDefinition[];
  runtime_actions: ExtensionRuntimeActionDefinition[];
  workspace_panels: ExtensionWorkspacePanelDefinition[];
  permissions: ExtensionPermissionDeclaration[];
}

export interface ExtensionRuntimeApi {
  invoke<TInput extends JsonValue, TOutput extends JsonValue>(actionKey: string, input: TInput): Promise<TOutput>;
}

export interface ExtensionLocalApi {
  getProfile(): Promise<JsonObject>;
}

export interface ExtensionApi {
  runtime: ExtensionRuntimeApi;
  local: ExtensionLocalApi;
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

export function createExtensionContext(api: ExtensionApi = createNoopApi()): ExtensionContext {
  const contributions: ExtensionContributions = {
    commands: [],
    flags: [],
    runtime_actions: [],
    workspace_panels: [],
    permissions: [],
  };
  return {
    api,
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

function createNoopApi(): ExtensionApi {
  return {
    runtime: {
      async invoke() {
        throw new Error("AgentDash runtime API is not connected");
      },
    },
    local: {
      async getProfile() {
        return {};
      },
    },
  };
}
