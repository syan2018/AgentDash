import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { SessionChatSubmitIntent } from "../../session";
import { extractPlatformEventData } from "../../session/model/platformEvent";
import {
  workspaceModulePresentationFromPlatformEventData,
  workspaceModulePresentedTabTarget,
} from "../../workspace-module/model/presentation";
import type {
  AgentRunSessionCommand,
  AgentRunSessionCommandState,
} from "./conversationCommandState";

export interface AgentRunWorkspacePanelTarget {
  typeId?: string;
  uri?: string;
  options?: { refreshContent?: boolean };
}

export interface AgentRunWorkspacePanelOpenPlan {
  target: AgentRunWorkspacePanelTarget;
  afterWorkspaceRefresh: boolean;
}

export interface AgentRunControlPlaneEffectPlan {
  refreshWorkspaceState?: boolean;
  refreshWorkspaceModuleCatalog?: boolean;
  refreshAgentRunListReason?: string;
  hookRuntimeRefresh?: {
    reason: string;
    immediate?: boolean;
  };
  openWorkspacePanel?: AgentRunWorkspacePanelOpenPlan;
}

export type AgentRunSubmitCommandResolution =
  | { ok: true; command: AgentRunSessionCommand }
  | { ok: false; message: string };

function commandLookupKey(command: AgentRunSessionCommand): string {
  return command.command_id;
}

export function buildAgentRunCommandLookup(
  commandState: AgentRunSessionCommandState,
): Map<string, AgentRunSessionCommand> {
  const lookup = new Map<string, AgentRunSessionCommand>();
  for (const command of commandState.commands.commands) {
    lookup.set(commandLookupKey(command), command);
  }
  if (commandState.localDraftAction) {
    lookup.set(commandLookupKey(commandState.localDraftAction), commandState.localDraftAction);
  }
  return lookup;
}

export function resolveAgentRunSubmitCommand(
  commandState: AgentRunSessionCommandState,
  intent: SessionChatSubmitIntent,
): AgentRunSubmitCommandResolution {
  const command = buildAgentRunCommandLookup(commandState).get(intent.command_id);
  if (!command) {
    return {
      ok: false,
      message: "当前 AgentRun 命令已刷新，请重试。",
    };
  }
  return { ok: true, command };
}

export function planAgentRunMessageSent(
  traceSessionId: string | null,
): AgentRunControlPlaneEffectPlan {
  return {
    refreshWorkspaceState: true,
    refreshAgentRunListReason: "message_sent",
    ...(traceSessionId
      ? { hookRuntimeRefresh: { reason: "message_sent", immediate: true } }
      : {}),
  };
}

export function planAgentRunTurnEnd(): AgentRunControlPlaneEffectPlan {
  return {
    refreshWorkspaceState: true,
    hookRuntimeRefresh: { reason: "turn_end", immediate: true },
    refreshAgentRunListReason: "turn_end",
  };
}

export function planAgentRunWorkspaceModuleOpened(): AgentRunControlPlaneEffectPlan {
  return {
    refreshWorkspaceState: true,
    refreshWorkspaceModuleCatalog: true,
    hookRuntimeRefresh: { reason: "workspace_module_user_opened" },
  };
}

function planWorkspaceModulePresented(
  event: BackboneEvent,
): AgentRunControlPlaneEffectPlan {
  const data = workspaceModulePresentationFromPlatformEventData(
    extractPlatformEventData(event),
  );
  const target = workspaceModulePresentedTabTarget(data);
  if (!target) return {};
  const refreshContent = target.typeId === "canvas" ? false : target.refreshRuntime;
  return {
    refreshWorkspaceState: target.refreshRuntime,
    refreshWorkspaceModuleCatalog: target.refreshRuntime,
    openWorkspacePanel: {
      afterWorkspaceRefresh: target.refreshRuntime,
      target: {
        typeId: target.typeId,
        uri: target.uri,
        options: { refreshContent },
      },
    },
  };
}

export function planAgentRunSystemEvent(
  eventType: string,
  event: BackboneEvent,
): AgentRunControlPlaneEffectPlan {
  switch (eventType) {
    case "hook_event":
    case "hook_action_resolved":
      return {
        hookRuntimeRefresh: { reason: eventType },
      };
    case "companion_dispatch_registered":
    case "companion_result_available":
    case "companion_result_returned":
    case "companion_human_request":
    case "companion_human_response":
    case "companion_review_request":
      return {
        refreshWorkspaceState: true,
        hookRuntimeRefresh: { reason: eventType },
      };
    case "context_frame": {
      const frameData = extractPlatformEventData(event);
      if (
        frameData?.kind === "capability_state_snapshot" ||
        frameData?.kind === "capability_state_delta"
      ) {
        return {
          refreshWorkspaceState: true,
          refreshWorkspaceModuleCatalog: true,
          hookRuntimeRefresh: { reason: eventType },
        };
      }
      return {};
    }
    case "session_meta_updated":
      return {
        refreshWorkspaceState: true,
        refreshAgentRunListReason: "session_meta_updated",
      };
    case "mailbox_state_changed":
      return {
        refreshWorkspaceState: true,
        refreshAgentRunListReason: "mailbox_state_changed",
      };
    case "workspace_module_presented":
      return planWorkspaceModulePresented(event);
    default:
      return {};
  }
}
