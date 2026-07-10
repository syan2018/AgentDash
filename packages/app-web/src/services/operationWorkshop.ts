import { api } from "../api/client";
import type {
  OperationWorkshopInvokeRequestDto,
  OperationWorkshopInvokeResponseDto,
  OperationWorkshopScriptPreflightRequestDto,
  OperationWorkshopScriptPreflightResponseDto,
  OperationWorkshopScriptRunRequestDto,
  OperationWorkshopScriptRunResponseDto,
  OperationWorkshopSurfaceDto,
  OperationWorkshopSurfaceRequestDto,
} from "../generated/interaction-contracts";

function workshopPath(projectId: string, suffix: string) {
  return `/projects/${encodeURIComponent(projectId)}/operation-workshop${suffix}`;
}

export function fetchOperationWorkshopSurface(
  projectId: string,
  request: OperationWorkshopSurfaceRequestDto,
) {
  return api.post<OperationWorkshopSurfaceDto>(workshopPath(projectId, "/surface"), request);
}

export function invokeWorkshopOperation(
  projectId: string,
  request: OperationWorkshopInvokeRequestDto,
) {
  return api.post<OperationWorkshopInvokeResponseDto>(workshopPath(projectId, "/invoke"), request);
}

export function preflightWorkshopOperationScript(
  projectId: string,
  request: OperationWorkshopScriptPreflightRequestDto,
) {
  return api.post<OperationWorkshopScriptPreflightResponseDto>(
    workshopPath(projectId, "/scripts/preflight"), request,
  );
}

export function runWorkshopOperationScript(
  projectId: string,
  request: OperationWorkshopScriptRunRequestDto,
) {
  return api.post<OperationWorkshopScriptRunResponseDto>(
    workshopPath(projectId, "/scripts/run"), request,
  );
}
