/**
 * Companion 用户请求交互卡片
 *
 * 渲染 companion_human_request 事件：
 * - 有 options → 按钮组
 * - 无 options → 文本输入 + 提交
 * - 已回应 → 显示回应结果
 */

import { useState } from "react";
import type { BackboneEvent } from "../../../generated/backbone-protocol";
import { extractPlatformEventData, isRecord } from "../model/platformEvent";
import { EventFullCard } from "./EventCards";
import { respondCompanionRequest } from "../../../services/executor";

export interface SessionCompanionRequestCardProps {
  event: BackboneEvent;
}

export function SessionCompanionRequestCard({ event }: SessionCompanionRequestCardProps) {
  const data = extractPlatformEventData(event);
  const payload = isRecord(data?.payload) ? data.payload : null;

  const requestId = typeof data?.request_id === "string" ? data.request_id : null;
  const gateId = stringField(data, "gate_id") ?? requestId;
  const payloadType = stringField(data, "payload_type") ?? stringField(payload, "type");
  const uiHint = stringField(data, "ui_hint");
  const prompt = stringField(data, "prompt") ?? stringField(payload, "prompt") ?? "Agent 请求你回应";
  const options = stringArrayField(data, "options");
  const wait = data?.wait === true;
  const requestedPaths = stringArrayField(payload, "requested_paths");
  const isCapabilityGrant =
    payloadType === "capability_grant_request" || uiHint === "capability_grant_card";

  const [isSubmitting, setIsSubmitting] = useState(false);
  const [responded, setResponded] = useState<string | null>(null);
  const [customInput, setCustomInput] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submitPayload = async (responsePayload: Record<string, unknown>, label: string) => {
    if (!gateId || isSubmitting) return;
    setError(null);
    setIsSubmitting(true);
    try {
      await respondCompanionRequest(gateId, responsePayload);
      setResponded(label);
    } catch (err) {
      setError(err instanceof Error ? err.message : "回应失败");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleRespond = async (choice: string) => {
    await submitPayload(
      {
        type: "decision",
        status: "approved",
        choice,
        summary: choice,
      },
      choice,
    );
  };

  const handleCapabilityGrant = async (approved: boolean) => {
    const status = approved ? "approved" : "rejected";
    await submitPayload(
      {
        type: "capability_grant_result",
        status,
        summary: approved ? "用户已批准临时能力申请" : "用户已拒绝临时能力申请",
        ...(approved ? { granted_paths: requestedPaths } : { rejected_paths: requestedPaths }),
      },
      approved ? "已批准" : "已拒绝",
    );
  };

  const handleSubmitCustom = async () => {
    const text = customInput.trim();
    if (!text) return;
    await handleRespond(text);
  };

  const detailLines: string[] = [];
  if (wait) {
    detailLines.push(
      responded
        ? "你的回应已经提交，session 已继续执行"
        : "Agent 正在等待你的回应（session 已挂起）",
    );
  }
  if (isCapabilityGrant) {
    const scope = stringField(payload, "scope");
    const reason = stringField(payload, "reason");
    const ttl = numberField(payload, "ttl_seconds");
    if (requestedPaths.length > 0) detailLines.push(`请求能力：${requestedPaths.join(", ")}`);
    if (reason) detailLines.push(`理由：${reason}`);
    if (scope) detailLines.push(`范围：${scope}${ttl ? `，TTL ${ttl} 秒` : ""}`);
  }

  const badge = wait
    ? "border-warning/25 bg-warning/10 text-warning"
    : "border-primary/25 bg-primary/8 text-primary";

  const bodyExtra = (
    <div className="space-y-2.5">
      {responded ? (
        <div className="flex items-center gap-2 rounded-[8px] border border-success/30 bg-success/10 px-3 py-2 text-sm text-success">
          <span>已回应：{responded}</span>
        </div>
      ) : (
        <>
          {isCapabilityGrant ? (
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={() => { void handleCapabilityGrant(true); }}
                disabled={isSubmitting}
                className="rounded-[8px] border border-success/30 bg-success/10 px-3 py-1.5 text-sm text-success transition-colors hover:bg-success/20 disabled:opacity-50"
              >
                {isSubmitting ? "处理中..." : "批准"}
              </button>
              <button
                type="button"
                onClick={() => { void handleCapabilityGrant(false); }}
                disabled={isSubmitting}
                className="rounded-[8px] border border-destructive/30 bg-destructive/10 px-3 py-1.5 text-sm text-destructive transition-colors hover:bg-destructive/20 disabled:opacity-50"
              >
                {isSubmitting ? "处理中..." : "拒绝"}
              </button>
            </div>
          ) : options.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {options.map((option) => (
                <button
                  key={option}
                  type="button"
                  onClick={() => { void handleRespond(option); }}
                  disabled={isSubmitting}
                  className="rounded-[8px] border border-primary/30 bg-primary/10 px-3 py-1.5 text-sm text-primary transition-colors hover:bg-primary/20 disabled:opacity-50"
                >
                  {isSubmitting ? "处理中..." : option}
                </button>
              ))}
            </div>
          ) : (
            <div className="flex gap-2">
              <input
                type="text"
                value={customInput}
                onChange={(e) => setCustomInput(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") void handleSubmitCustom(); }}
                placeholder="输入回应…"
                disabled={isSubmitting}
                className="min-w-0 flex-1 rounded-[8px] border border-border bg-background px-3 py-1.5 text-sm outline-none focus:border-primary/50 disabled:opacity-50"
              />
              <button
                type="button"
                onClick={() => { void handleSubmitCustom(); }}
                disabled={isSubmitting || !customInput.trim()}
                className="rounded-[8px] border border-primary/30 bg-primary/10 px-3 py-1.5 text-sm text-primary transition-colors hover:bg-primary/20 disabled:opacity-50"
              >
                {isSubmitting ? "处理中..." : "提交"}
              </button>
            </div>
          )}
        </>
      )}
      {error && (
        <div className="rounded-[8px] border border-destructive/30 bg-destructive/10 p-2 text-sm text-destructive">
          {error}
        </div>
      )}
    </div>
  );

  return (
    <EventFullCard
      badgeToken={isCapabilityGrant ? "能力申请" : wait ? "等待回应" : "请求"}
      badgeClass={badge}
      message={prompt}
      detailLines={detailLines}
      bodyExtra={bodyExtra}
      debugChips={[
        ...(requestId ? [`request: ${requestId.slice(0, 12)}`] : []),
        ...(gateId && gateId !== requestId ? [`gate: ${gateId.slice(0, 12)}`] : []),
        ...(payloadType ? [`payload: ${payloadType}`] : []),
      ]}
    />
  );
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
