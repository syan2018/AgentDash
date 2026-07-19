import type { ManagedRuntimePlatformChange } from "../../../generated/agent-runtime-validators";
import type { ProjectEventStreamEnvelope } from "../../../generated/project-contracts";
import type { WorkspaceModulePresentation } from "../../../generated/workspace-module-contracts";
import type { WorkspaceModulePresentationIntent } from "../../../generated/agent-run-product-projection-contracts";
import {
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

export function planAgentRunRuntimeChanges(
  changes: readonly ManagedRuntimePlatformChange[],
): AgentRunLiveEventPlan {
  const effects: AgentRunControlPlaneEffectPlan = {};
  let refreshTaskPlan = false;

  for (const change of changes) {
    const delta = change.delta;
    if (delta.kind === "source_projection_changed") {
      effects.refreshWorkspaceState = true;
      effects.refreshAgentRunListReason = "managed_runtime_projection_changed";
      if (
        delta.section === "snapshot"
        || delta.section === "lifecycle"
        || delta.section === "turns"
        || delta.section === "items"
        || delta.section === "interactions"
      ) {
        refreshTaskPlan = true;
      }
      if (delta.section === "surface") {
        effects.hookRuntimeRefresh = {
          reason: "managed_runtime_surface_changed",
        };
      }
      continue;
    }
    if (
      delta.kind === "runtime_lifecycle_changed"
      || delta.kind === "source_binding_changed"
    ) {
      effects.refreshWorkspaceState = true;
      effects.refreshAgentRunListReason = "managed_runtime_state_changed";
      refreshTaskPlan = delta.kind === "runtime_lifecycle_changed"
        || refreshTaskPlan;
      continue;
    }
    if (delta.kind === "surface_evidence_changed") {
      effects.refreshWorkspaceState = true;
      effects.hookRuntimeRefresh = {
        reason: "managed_runtime_surface_changed",
      };
      continue;
    }
    if (delta.kind === "operation_upserted") {
      effects.refreshAgentRunListReason = "managed_runtime_operation_changed";
    }
  }

  return { effects, refreshTaskPlan };
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
    || change.projection !== "agent_run_list"
    || change.reason !== "title_changed"
  ) {
    return {};
  }
  return {
    refreshWorkspaceState: true,
    refreshAgentRunListReason: "title_changed",
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
