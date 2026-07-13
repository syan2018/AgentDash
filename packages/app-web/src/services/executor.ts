import { api } from "../api/client";
import type { PermissionPolicy } from "../features/executor-selector/model/types";
import type { ProjectAgentExecutor } from "../generated/project-agent-contracts";
import type { CompanionGateRespondResponse } from "../generated/companion-contracts";
import {
  approveAgentRunToolCall,
  rejectAgentRunToolCall,
  type AgentRunRuntimeTarget,
} from "./agentRunRuntime";

export type ExecutorProfile = string;

export type { PermissionPolicy };

export type ExecutorConfig = ProjectAgentExecutor;

export async function approveToolCallForAgentRun(
  target: AgentRunRuntimeTarget,
  toolCallId: string,
) {
  return approveAgentRunToolCall(target, toolCallId);
}

export async function rejectToolCallForAgentRun(
  target: AgentRunRuntimeTarget,
  toolCallId: string,
  reason?: string,
) {
  return rejectAgentRunToolCall(target, toolCallId, reason);
}

export async function respondCompanionRequest(
  gateId: string,
  payload: Record<string, unknown>,
): Promise<CompanionGateRespondResponse> {
  return api.post<CompanionGateRespondResponse>(
    `/companion-gates/${encodeURIComponent(gateId)}/respond`,
    { payload },
  );
}
