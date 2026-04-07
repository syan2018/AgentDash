import { api } from "../api/client";
import type { ContentBlock } from "@agentclientprotocol/sdk";
import type { ThinkingLevel } from "../types";
import type { PermissionPolicy } from "../features/executor-selector/model/types";

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

export interface PromptSessionRequest {
  promptBlocks: ContentBlock[];
  workingDir?: string;
  env?: Record<string, string>;
  executorConfig?: ExecutorConfig;
}

export async function promptSession(sessionId: string, req: PromptSessionRequest): Promise<void> {
  await api.post<void>(
    `/sessions/${encodeURIComponent(sessionId)}/prompt`,
    req,
  );
}

export async function approveToolCall(sessionId: string, toolCallId: string): Promise<void> {
  await api.post<void>(
    `/sessions/${encodeURIComponent(sessionId)}/tool-approvals/${encodeURIComponent(toolCallId)}/approve`,
    {},
  );
}

export async function rejectToolCall(
  sessionId: string,
  toolCallId: string,
  reason?: string,
): Promise<void> {
  await api.post<void>(
    `/sessions/${encodeURIComponent(sessionId)}/tool-approvals/${encodeURIComponent(toolCallId)}/reject`,
    { reason },
  );
}

export async function respondCompanionRequest(
  sessionId: string,
  requestId: string,
  payload: Record<string, unknown>,
): Promise<void> {
  await api.post<void>(
    `/sessions/${encodeURIComponent(sessionId)}/companion-requests/${encodeURIComponent(requestId)}/respond`,
    { payload },
  );
}
