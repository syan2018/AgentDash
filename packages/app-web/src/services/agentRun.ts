import { api } from "../api/client";
import type { DeleteAgentRunResponse } from "../generated/workflow-contracts";

export async function deleteAgentRun(
  projectId: string,
  runId: string,
): Promise<DeleteAgentRunResponse> {
  return api.delete<DeleteAgentRunResponse>(
    `/projects/${encodeURIComponent(projectId)}/agent-runs/${encodeURIComponent(runId)}`,
  );
}
