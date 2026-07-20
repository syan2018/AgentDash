import type {
  BackboneEvent,
} from "../../../generated/backbone-protocol";
import type {
  ControlPlaneProjectionChanged,
  ProjectEventStreamEnvelope,
} from "../../../generated/project-contracts";
import type { WorkspaceModulePresentation } from "../../../generated/workspace-module-contracts";
import type { WorkspaceModulePresentationIntent } from "../../../generated/agent-run-product-projection-contracts";
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
  hookRuntimeRefresh?: {
    reason: string;
    immediate?: boolean;
  };
  openWorkspacePanel?: AgentRunWorkspacePanelOpenPlan;
}

export interface AgentRunLiveEventPlan {
  effects: AgentRunControlPlaneEffectPlan;
  refreshTaskPlan: boolean;
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

export function planAgentRunWorkspaceModuleOpened(): AgentRunControlPlaneEffectPlan {
  return {
    refreshWorkspaceState: true,
    hookRuntimeRefresh: { reason: "workspace_module_user_opened" },
  };
}

function projectionRefreshReason(change: ControlPlaneProjectionChanged): string {
  return "control_plane:" + change.projection + ":" + change.reason;
}

function planWorkspaceModulePresentationPayload(
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

export function planWorkspaceModulePresentationIntent(
  intent: WorkspaceModulePresentationIntent,
): AgentRunControlPlaneEffectPlan {
  return planWorkspaceModulePresentationPayload(intent.presentation);
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
  if (change.reason === "title_changed") {
    plan.refreshWorkspaceState = true;
  }

  if (
    change.reason === "hook_effect_applied" ||
    change.reason === "hook_auto_resume_queued"
  ) {
    plan.hookRuntimeRefresh = { reason };
  }

  return plan;
}

function isTaskPlanMutation(event: BackboneEvent): boolean {
  if (event.type !== "item_completed") return false;
  const item = event.payload.item;
  return item.type === "dynamicToolCall" &&
    item.tool === "task_write" &&
    item.status === "completed" &&
    item.success !== false;
}

function isTurnTerminal(event: BackboneEvent): boolean {
  if (event.type === "turn_completed") return true;
  if (
    event.type !== "platform" ||
    event.payload.kind !== "session_meta_update" ||
    event.payload.data.key !== "turn_terminal"
  ) {
    return false;
  }
  const value = event.payload.data.value;
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  return value.terminal_type === "turn_completed" ||
    value.terminal_type === "turn_failed" ||
    value.terminal_type === "turn_interrupted";
}

function planAgentRunEventEffects(
  event: BackboneEvent,
): AgentRunControlPlaneEffectPlan {
  if (isTurnTerminal(event)) {
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

  if (event.payload.kind === "context_frame_changed") {
    return {
      refreshWorkspaceState: true,
      hookRuntimeRefresh: { reason: "context_frame_changed" },
    };
  }
  if (event.payload.kind === "hook_trace") {
    return {
      hookRuntimeRefresh: { reason: "hook_event" },
    };
  }
  if (
    event.payload.kind === "session_meta_update" &&
    event.payload.data.key === "hook_action_resolved"
  ) {
    return {
      hookRuntimeRefresh: { reason: "hook_action_resolved" },
    };
  }
  return {};
}

export function planAgentRunLiveEvent(
  event: BackboneEvent,
): AgentRunLiveEventPlan {
  return {
    effects: planAgentRunEventEffects(event),
    refreshTaskPlan: isTurnTerminal(event) || isTaskPlanMutation(event),
  };
}
