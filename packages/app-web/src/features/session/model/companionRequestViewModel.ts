import type { BackboneEvent } from "../../../generated/backbone-protocol";
import { extractPlatformEventData, isRecord } from "./platformEvent";

export interface CompanionCapabilityGrantViewModel {
  requestedPaths: string[];
  reason: string | null;
  scope: string | null;
  ttlSeconds: number | null;
}

export interface CompanionRequestViewModel {
  requestId: string | null;
  gateId: string | null;
  payloadType: string | null;
  uiHint: string | null;
  prompt: string;
  options: string[];
  wait: boolean;
  isCapabilityGrant: boolean;
  capabilityGrant: CompanionCapabilityGrantViewModel;
  badgeToken: string;
  badgeClass: string;
  debugChips: string[];
}

export interface CompanionRequestSubmission {
  payload: Record<string, unknown>;
  label: string;
}

export function parseCompanionRequest(event: BackboneEvent): CompanionRequestViewModel {
  const data = extractPlatformEventData(event);
  const payload = isRecord(data?.payload) ? data.payload : null;

  const requestId = typeof data?.request_id === "string" ? data.request_id : null;
  const gateId = stringField(data, "gate_id") ?? requestId;
  const payloadType = stringField(data, "payload_type") ?? stringField(payload, "type");
  const uiHint = stringField(data, "ui_hint");
  const prompt = stringField(data, "prompt") ?? stringField(payload, "prompt") ?? "Agent 请求你回应";
  const options = stringArrayField(data, "options");
  const wait = data?.wait === true;
  const isCapabilityGrant =
    payloadType === "capability_grant_request" || uiHint === "capability_grant_card";

  const capabilityGrant: CompanionCapabilityGrantViewModel = {
    requestedPaths: stringArrayField(payload, "requested_paths"),
    reason: stringField(payload, "reason"),
    scope: stringField(payload, "scope"),
    ttlSeconds: numberField(payload, "ttl_seconds"),
  };

  return {
    requestId,
    gateId,
    payloadType,
    uiHint,
    prompt,
    options,
    wait,
    isCapabilityGrant,
    capabilityGrant,
    badgeToken: isCapabilityGrant ? "能力申请" : wait ? "等待回应" : "请求",
    badgeClass: wait
      ? "border-warning/25 bg-warning/10 text-warning"
      : "border-primary/25 bg-primary/8 text-primary",
    debugChips: buildDebugChips(requestId, gateId, payloadType),
  };
}

export function buildCompanionRequestDetailLines(
  request: CompanionRequestViewModel,
  responded: string | null,
): string[] {
  const detailLines: string[] = [];
  if (request.wait) {
    detailLines.push(
      responded
        ? "你的回应已经提交，session 已继续执行"
        : "Agent 正在等待你的回应（session 已挂起）",
    );
  }

  if (request.isCapabilityGrant) {
    const { requestedPaths, reason, scope, ttlSeconds } = request.capabilityGrant;
    if (requestedPaths.length > 0) detailLines.push(`请求能力：${requestedPaths.join(", ")}`);
    if (reason) detailLines.push(`理由：${reason}`);
    if (scope) detailLines.push(`范围：${scope}${ttlSeconds ? `，TTL ${ttlSeconds} 秒` : ""}`);
    detailLines.push("能力授权以 PermissionGrant 审批为准，此会话卡片不提交授权结果");
  }

  return detailLines;
}

export function buildCompanionChoiceSubmission(choice: string): CompanionRequestSubmission {
  return {
    payload: {
      type: "decision",
      status: "approved",
      choice,
      summary: choice,
    },
    label: choice,
  };
}

function buildDebugChips(
  requestId: string | null,
  gateId: string | null,
  payloadType: string | null,
): string[] {
  return [
    ...(requestId ? [`request: ${requestId.slice(0, 12)}`] : []),
    ...(gateId && gateId !== requestId ? [`gate: ${gateId.slice(0, 12)}`] : []),
    ...(payloadType ? [`payload: ${payloadType}`] : []),
  ];
}

function stringField(record: Record<string, unknown> | null | undefined, key: string): string | null {
  const value = record?.[key];
  return typeof value === "string" && value.trim() ? value : null;
}

function numberField(record: Record<string, unknown> | null | undefined, key: string): number | null {
  const value = record?.[key];
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function stringArrayField(record: Record<string, unknown> | null | undefined, key: string): string[] {
  const value = record?.[key];
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string" && item.trim().length > 0);
}
