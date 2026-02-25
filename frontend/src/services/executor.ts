export type ExecutorProfile = string;

export type PermissionPolicy = "plan" | "supervised" | "auto";

export interface ExecutorConfig {
  executor: ExecutorProfile;
  variant?: string;
  modelId?: string;
  agentId?: string;
  reasoningId?: string;
  permissionPolicy?: PermissionPolicy;
}

export interface PromptSessionRequest {
  prompt: string;
  workingDir?: string;
  env?: Record<string, string>;
  executorConfig?: ExecutorConfig;
}

export async function promptSession(sessionId: string, req: PromptSessionRequest): Promise<void> {
  const res = await fetch(`/api/sessions/${encodeURIComponent(sessionId)}/prompt`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `promptSession failed: HTTP ${res.status}`);
  }
}

