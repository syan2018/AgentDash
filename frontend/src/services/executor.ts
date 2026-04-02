import { buildApiPath } from "../api/origin";
import { authenticatedFetch } from "../api/client";
import type { ContentBlock } from "@agentclientprotocol/sdk";
import type { ThinkingLevel } from "../types";
import type { PermissionPolicy } from "../features/executor-selector/model/types";

export type ExecutorProfile = string;

export type { PermissionPolicy };

export interface ExecutorConfig {
  executor: ExecutorProfile;
  provider_id?: string;
  // 对齐后端 ExecutorConfig（Rust 侧使用 snake_case 字段）
  model_id?: string;
  agent_id?: string;
  /** 推理级别，替代旧的 reasoning_id 字段 */
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
  const res = await authenticatedFetch(buildApiPath(`/sessions/${encodeURIComponent(sessionId)}/prompt`), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `promptSession failed: HTTP ${res.status}`);
  }
}

export async function approveToolCall(sessionId: string, toolCallId: string): Promise<void> {
  const res = await authenticatedFetch(
    buildApiPath(
      `/sessions/${encodeURIComponent(sessionId)}/tool-approvals/${encodeURIComponent(toolCallId)}/approve`,
    ),
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
    },
  );

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `approveToolCall failed: HTTP ${res.status}`);
  }
}

export async function rejectToolCall(
  sessionId: string,
  toolCallId: string,
  reason?: string,
): Promise<void> {
  const res = await authenticatedFetch(
    buildApiPath(
      `/sessions/${encodeURIComponent(sessionId)}/tool-approvals/${encodeURIComponent(toolCallId)}/reject`,
    ),
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ reason }),
    },
  );

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `rejectToolCall failed: HTTP ${res.status}`);
  }
}
