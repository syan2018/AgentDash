import {
  fetchAgentRunTerminalChanges,
  fetchAgentRunTerminalSnapshot,
  fetchWorkspacePresentationChanges,
  fetchWorkspacePresentationSnapshot,
} from "../../../services/agentRunProductProjections";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import type {
  AgentRunTerminalChange,
  AgentRunTerminalSnapshot,
  WorkspaceModulePresentationChange,
  WorkspaceModulePresentationSnapshot,
} from "../../../generated/agent-run-product-projection-contracts";
import {
  connectProductProjectionFeed,
  type ProductProjectionFeedConnection,
  type ProductProjectionFeedObserver,
} from "./productProjectionFeed";

const POLL_INTERVAL_MS = 500;

function schedule(callback: () => void): ReturnType<typeof setTimeout> {
  return setTimeout(callback, POLL_INTERVAL_MS);
}

function cancel(handle: unknown): void {
  clearTimeout(handle as ReturnType<typeof setTimeout>);
}

export function connectWorkspacePresentationFeed(
  target: AgentRunRuntimeTarget,
  observer: ProductProjectionFeedObserver<
    WorkspaceModulePresentationSnapshot,
    WorkspaceModulePresentationChange
  >,
): ProductProjectionFeedConnection {
  return connectProductProjectionFeed(target, observer, {
    fetchSnapshot: fetchWorkspacePresentationSnapshot,
    fetchChanges: fetchWorkspacePresentationChanges,
    schedule,
    cancel,
  });
}

export function connectAgentRunTerminalFeed(
  target: AgentRunRuntimeTarget,
  observer: ProductProjectionFeedObserver<
    AgentRunTerminalSnapshot,
    AgentRunTerminalChange
  >,
): ProductProjectionFeedConnection {
  return connectProductProjectionFeed(target, observer, {
    fetchSnapshot: fetchAgentRunTerminalSnapshot,
    fetchChanges: fetchAgentRunTerminalChanges,
    schedule,
    cancel,
  });
}
