import type {
  BackboneEvent,
} from "../../../generated/backbone-protocol";
import type {
  ControlPlaneProjectionChanged,
  ProjectEventStreamEnvelope,
} from "../../../generated/project-contracts";
import type { WorkspaceModulePresentation } from "../../../generated/backbone-protocol";
import { workspaceModulePresentationTabTarget } from "../../workspace-module/model/presentation";
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
  openWorkspacePanel?: AgentRunWorkspacePanelOpenPlan;
}

export interface AgentRunLiveEventPlan {
  effects: AgentRunControlPlaneEffectPlan;
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

export function planAgentRunProjectEvent(
  event: ProjectEventStreamEnvelope,
  target: { runId: string; agentId: string },
): AgentRunControlPlaneEffectPlan {
  if (event.type !== "ControlPlaneProjectionChanged") return {};
  const change = event.data.change;
  if (
    change.run_id !== target.runId
    || change.agent_id !== target.agentId
  ) {
    return {};
  }
  return planControlPlaneProjectionChanged(change);
}

export function planAgentRunTurnEnded(): AgentRunControlPlaneEffectPlan {
  return {
    refreshWorkspaceState: true,
    refreshAgentRunListReason: "turn_ended",
  };
}

export function planAgentRunTurnStarted(): AgentRunControlPlaneEffectPlan {
  return {
    refreshWorkspaceState: true,
    refreshAgentRunListReason: "turn_started",
  };
}

function projectionRefreshReason(change: ControlPlaneProjectionChanged): string {
  return "control_plane:" + change.projection + ":" + change.reason;
}

export function planWorkspaceModulePresentationPayload(
  data: WorkspaceModulePresentation | null,
): AgentRunControlPlaneEffectPlan {
  if (!data) return {};
  const target = workspaceModulePresentationTabTarget(data);
  if (!target) return {};
  return {
    refreshWorkspaceState: true,
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
      break;
  }

  if (change.reason === "title_changed") {
    plan.refreshWorkspaceState = true;
  }

  return plan;
}

function planAgentRunEventEffects(
  event: BackboneEvent,
): AgentRunControlPlaneEffectPlan {
  if (event.type === "turn_started") {
    return planAgentRunTurnStarted();
  }
  if (event.type === "turn_completed") {
    return planAgentRunTurnEnded();
  }
  if (event.type === "thread_name_updated") {
    return {
      refreshWorkspaceState: true,
      refreshAgentRunListReason: "thread_name_updated",
    };
  }
  if (event.type !== "platform") {
    return {};
  }

  if (event.payload.kind === "workspace_module_presentation_requested") {
    return planWorkspaceModulePresentationPayload(event.payload.data);
  }
  return {};
}

export function planAgentRunLiveEvent(
  event: BackboneEvent,
): AgentRunLiveEventPlan {
  return {
    effects: planAgentRunEventEffects(event),
  };
}
