import { api } from "../api/client";
import type { ProjectAgentExecutor } from "../generated/project-agent-contracts";
import type { CompanionGateRespondResponse } from "../generated/companion-contracts";

export type ExecutorProfile = string;

export type ExecutorConfig = ProjectAgentExecutor;

export async function respondCompanionRequest(
  gateId: string,
  payload: Record<string, unknown>,
): Promise<CompanionGateRespondResponse> {
  return api.post<CompanionGateRespondResponse>(
    `/companion-gates/${encodeURIComponent(gateId)}/respond`,
    { payload },
  );
}
