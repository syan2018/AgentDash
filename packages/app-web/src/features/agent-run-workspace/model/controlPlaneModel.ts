import type {
  BackboneEvent,
  ControlPlaneProjectionChanged,
} from "../../../generated/backbone-protocol";
import type { WorkspaceModulePresentation } from "../../../generated/workspace-module-contracts";
import {
  workspaceModulePresentationFromPlatformEventData,
  workspaceModulePresentationTabTarget,
} from "../../workspace-module/model/presentation";
import type {
  AgentRunConversationCommand,
  AgentRunConversationCommandState,
  AgentRunChatSubmitIntent,
} from "./conversationCommandState";

export interface AgentRunWorkspacePanelTarget {
  typeId?: string;
  uri?: string;
  options?: { refreshContent?: boolean };
}
export interface AgentRunWorkspacePanelOpenPlan {
  target: AgentRunWorkspacePanelTarget;
  afterWorkspaceRefresh: boolean;
  presentation: WorkspaceModulePresentation;
}

export interface AgentRunControlPlaneEffectPlan {
  refreshWorkspaceState?: boolean;
  refreshAgentRunListReason?: string;
  hookRuntimeRefresh?: {
    reason: string;
    immediate?: boolean;
  };
  openWorkspacePanel?: AgentRunWorkspacePanelOpenPlan;
}

export type AgentRunSubmitCommandResolution =
  | { ok: true; command: AgentRunConversationCommand }
  | { ok: false; message: string };

function commandLookupKey(command: AgentRunConversationCommand): string {
  return command.command_id;
}

export function buildAgentRunCommandLookup(
  commandState: AgentRunConversationCommandState,
): Map<string, AgentRunConversationCommand> {
  const lookup = new Map<string, AgentRunConversationCommand>();
  for (const command of commandState.commands.commands) {
    lookup.set(commandLookupKey(command), command);
  }
  if (commandState.localDraftAction) {
    lookup.set(commandLookupKey(commandState.localDraftAction), commandState.localDraftAction);
  }
  return lookup;
}

export function resolveAgentRunSubmitCommand(
  commandState: AgentRunConversationCommandState,
  intent: AgentRunChatSubmitIntent,
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

export function planAgentRunMessageSent(): AgentRunControlPlaneEffectPlan {
  return {
    refreshWorkspaceState: true,
    refreshAgentRunListReason: "message_sent",
    hookRuntimeRefresh: { reason: "message_sent", immediate: true },
  };
}

export function planAgentRunTurnEnded(): AgentRunControlPlaneEffectPlan {
  return {
    refreshWorkspaceState: true,
    refreshAgentRunListReason: "turn_ended",
  };
}

export function planAgentRunWorkspaceModuleOpened(): AgentRunControlPlaneEffectPlan {
  return {
    refreshWorkspaceState: true,
    hookRuntimeRefresh: { reason: "workspace_module_user_opened" },
  };
}

function projectionRefreshReason(change: ControlPlaneProjectionChanged): string {
  return "control_plane:" + change.projection + ":" + change.reason;
}

function extractControlPlaneProjectionChanged(
  event: BackboneEvent,
): ControlPlaneProjectionChanged | null {
  if (event.type !== "platform") return null;
  if (event.payload.kind !== "control_plane_projection_changed") return null;
  return event.payload.data;
}

function planWorkspaceModulePresentation(
  change: ControlPlaneProjectionChanged,
): AgentRunControlPlaneEffectPlan {
  const data = workspaceModulePresentationFromPlatformEventData(
    change.workspace_module_presentation,
  );
  if (!data) return {};
  const target = workspaceModulePresentationTabTarget(data);
  if (!target) return {};
  return {
    openWorkspacePanel: {
      afterWorkspaceRefresh: true,
      presentation: data,
      target: {
        typeId: target.typeId,
        uri: target.uri,
        options: { refreshContent: false },
      },
    },
  };
}

function planControlPlaneProjectionChanged(
  change: ControlPlaneProjectionChanged,
): AgentRunControlPlaneEffectPlan {
  const reason = projectionRefreshReason(change);
  const plan: AgentRunControlPlaneEffectPlan = {};

  switch (change.projection) {
    case "workspace":
    case "mailbox":
    case "waiting":
    case "delivery":
    case "title":
      plan.refreshWorkspaceState = true;
      plan.refreshAgentRunListReason = reason;
      break;
    case "agent_run_list":
      plan.refreshAgentRunListReason = reason;
      break;
    case "resource_surface":
      plan.refreshWorkspaceState = true;
      break;
    case "hook_runtime":
      plan.hookRuntimeRefresh = { reason };
      break;
  }

  if (
    change.reason === "capability_state_changed" ||
    change.reason === "context_frame_changed"
  ) {
    plan.refreshWorkspaceState = true;
    plan.hookRuntimeRefresh = { reason };
  }

  if (
    change.reason === "hook_effect_applied" ||
    change.reason === "hook_auto_resume_queued"
  ) {
    plan.hookRuntimeRefresh = { reason };
  }

  if (change.workspace_module_presentation) {
    return {
      ...plan,
      ...planWorkspaceModulePresentation(change),
    };
  }

  return plan;
}

export function planAgentRunSystemEvent(
  eventType: string,
  event: BackboneEvent,
): AgentRunControlPlaneEffectPlan {
  const controlPlaneChange = extractControlPlaneProjectionChanged(event);
  if (controlPlaneChange) {
    return planControlPlaneProjectionChanged(controlPlaneChange);
  }
  if (event.type === "platform" && event.payload.kind === "context_frame_changed") {
    return {
      refreshWorkspaceState: true,
      hookRuntimeRefresh: { reason: "context_frame_changed" },
    };
  }

  switch (eventType) {
    case "thread_name_updated":
      return {
        refreshWorkspaceState: true,
        refreshAgentRunListReason: "thread_name_updated",
      };
    case "hook_event":
    case "hook_action_resolved":
      return {
        hookRuntimeRefresh: { reason: eventType },
      };
    default:
      return {};
  }
}
