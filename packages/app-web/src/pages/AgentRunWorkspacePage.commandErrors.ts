export function apiErrorCode(error: unknown): string | null {
  if (!error || typeof error !== "object" || !("errorCode" in error)) return null;
  return typeof error.errorCode === "string" ? error.errorCode : null;
}

export function isStaleAgentRunCommandError(error: unknown): boolean {
  return apiErrorCode(error) === "stale_command";
}

export function silentCommandRefreshError(): Error {
  const error = new Error("AgentRun workspace state refreshed.");
  (error as { silentCommandRefresh?: boolean }).silentCommandRefresh = true;
  return error;
}
