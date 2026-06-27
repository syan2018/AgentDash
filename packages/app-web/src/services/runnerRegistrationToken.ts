import { api } from "../api/client";
import type {
  RunnerRegistrationTokenCreateRequest,
  RunnerRegistrationTokenCreateResponse,
  RunnerRegistrationTokenMetadataResponse,
  RunnerRegistrationTokenRevokeResponse,
  RunnerRegistrationTokenRotateResponse,
} from "../generated/backend-contracts";

export type RunnerRegistrationToken = RunnerRegistrationTokenMetadataResponse;
export type CreateRunnerRegistrationTokenPayload = RunnerRegistrationTokenCreateRequest;
export type RunnerRegistrationTokenCreateResult = RunnerRegistrationTokenCreateResponse;
export type RunnerRegistrationTokenRotateResult = RunnerRegistrationTokenRotateResponse;

function tokensBasePath(projectId: string): string {
  return `/projects/${projectId}/runner-registration-tokens`;
}

export function listRunnerRegistrationTokens(
  projectId: string,
): Promise<RunnerRegistrationToken[]> {
  return api.get<RunnerRegistrationToken[]>(tokensBasePath(projectId));
}

export function createRunnerRegistrationToken(
  projectId: string,
  payload: CreateRunnerRegistrationTokenPayload,
): Promise<RunnerRegistrationTokenCreateResult> {
  return api.post<RunnerRegistrationTokenCreateResult>(tokensBasePath(projectId), payload);
}

export function revokeRunnerRegistrationToken(
  projectId: string,
  tokenId: string,
): Promise<RunnerRegistrationTokenRevokeResponse> {
  return api.post<RunnerRegistrationTokenRevokeResponse>(
    `${tokensBasePath(projectId)}/${tokenId}/revoke`,
    {},
  );
}

export function rotateRunnerRegistrationToken(
  projectId: string,
  tokenId: string,
): Promise<RunnerRegistrationTokenRotateResult> {
  return api.post<RunnerRegistrationTokenRotateResult>(
    `${tokensBasePath(projectId)}/${tokenId}/rotate`,
    {},
  );
}
