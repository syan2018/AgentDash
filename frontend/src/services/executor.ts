import { buildApiPath } from "../api/origin";

export type ExecutorProfile = string;

export type PermissionPolicy = "AUTO" | "SUPERVISED" | "PLAN";

export interface ExecutorConfig {
  executor: ExecutorProfile;
  variant?: string;
  // 对齐 vibe-kanban / executors::profile::ExecutorConfig（Rust 侧使用 snake_case 字段）
  model_id?: string;
  agent_id?: string;
  reasoning_id?: string;
  permission_policy?: PermissionPolicy;
}

export interface PromptSessionRequest {
  prompt?: string;
  promptBlocks?: unknown[];
  workingDir?: string;
  env?: Record<string, string>;
  executorConfig?: ExecutorConfig;
}

export async function promptSession(sessionId: string, req: PromptSessionRequest): Promise<void> {
  const res = await fetch(buildApiPath(`/sessions/${encodeURIComponent(sessionId)}/prompt`), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `promptSession failed: HTTP ${res.status}`);
  }
}

