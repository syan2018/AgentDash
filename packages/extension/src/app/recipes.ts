import type {
  AgentDashBackendServiceOptions,
  AgentDashCapabilityRecipe,
  AgentDashCustomChannelOptions,
  AgentDashHttpProxyOptions,
  AgentDashLocalCommandOptions,
  AgentDashWorkspaceFilesOptions,
} from "./types.js";

export function httpProxy(options: AgentDashHttpProxyOptions): AgentDashCapabilityRecipe {
  return { kind: "http_proxy", options };
}

export function localCommand(options: AgentDashLocalCommandOptions): AgentDashCapabilityRecipe {
  return { kind: "local_command", options };
}

export function workspaceFiles(options: AgentDashWorkspaceFilesOptions = {}): AgentDashCapabilityRecipe {
  return { kind: "workspace_files", options };
}

export function customChannel(options: AgentDashCustomChannelOptions): AgentDashCapabilityRecipe {
  return { kind: "custom_channel", options };
}

export function backendService(options: AgentDashBackendServiceOptions): AgentDashCapabilityRecipe {
  return { kind: "backend_service", options };
}
