import type { BackendConfig, BackendRuntimeSummary } from "../types";

export function applyBackendRuntimeSummaries(
  backends: BackendConfig[],
  runtimeSummaries: BackendRuntimeSummary[],
): BackendConfig[] {
  if (runtimeSummaries.length === 0) return backends;
  const summaries = new Map(runtimeSummaries.map((summary) => [summary.backend_id, summary]));

  return backends.map((backend) => {
    const summary = summaries.get(backend.id);
    if (!summary) return backend;
    return {
      ...backend,
      online: backend.online || summary.online,
      runtime_health: backend.runtime_health ?? summary.runtime_health,
      capabilities: backend.capabilities ?? capabilitiesFromRuntimeSummary(summary),
    };
  });
}

export function backendAvailabilitySignature(
  backends: BackendConfig[],
  runtimeSummaries: BackendRuntimeSummary[],
): string {
  const summaries = new Map(runtimeSummaries.map((summary) => [summary.backend_id, summary]));
  return backends
    .map((backend) => {
      const summary = summaries.get(backend.id);
      return [
        backend.id,
        backend.online || summary?.online ? "online" : "offline",
        backend.runtime_health?.status ?? summary?.runtime_health?.status ?? "",
        backend.runtime_health?.updated_at ?? summary?.runtime_health?.updated_at ?? "",
      ].join(":");
    })
    .join("|");
}

function capabilitiesFromRuntimeSummary(summary: BackendRuntimeSummary): BackendConfig["capabilities"] {
  return {
    executors: summary.executors.map((executor) => ({
      id: executor.executor_id,
      name: executor.name,
      variants: executor.variants,
      available: executor.available,
    })),
    supports_cancel: false,
    supports_discover_options: false,
    mcp_servers: [],
  };
}
