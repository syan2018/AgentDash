import { api } from "../api/client";
import type { ThinkingLevel } from "../types";
import type { PermissionPolicy } from "../features/executor-selector/model/types";
import type { CompanionGateRespondResponse } from "../generated/companion-contracts";
import {
  approveAgentRunToolCall,
  rejectAgentRunToolCall,
  type AgentRunRuntimeTarget,
} from "./agentRunRuntime";

export type ExecutorProfile = string;

export type { PermissionPolicy };

export interface ExecutorConfig {
  executor: ExecutorProfile;
  provider_id?: string;
  model_id?: string;
  agent_id?: string;
  thinking_level?: ThinkingLevel;
  permission_policy?: PermissionPolicy;
}

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
