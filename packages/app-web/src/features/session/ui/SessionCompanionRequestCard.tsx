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
import { respondCompanionRequest } from "../../../services/executor";
import {
  buildCompanionChoiceSubmission,
  buildCompanionRequestDetailLines,
  parseCompanionRequest,
} from "../model/companionRequestViewModel";
import { EventFullCard } from "./EventCards";

export interface SessionCompanionRequestCardProps {
  event: BackboneEvent;
}

export function SessionCompanionRequestCard({ event }: SessionCompanionRequestCardProps) {
  const request = parseCompanionRequest(event);

  const [isSubmitting, setIsSubmitting] = useState(false);
  const [responded, setResponded] = useState<string | null>(null);
  const [customInput, setCustomInput] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submitPayload = async (responsePayload: Record<string, unknown>, label: string) => {
    if (!request.gateId || isSubmitting) return;
    setError(null);
    setIsSubmitting(true);
    try {
      await respondCompanionRequest(request.gateId, responsePayload);
      setResponded(label);
    } catch (err) {
      setError(err instanceof Error ? err.message : "回应失败");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleRespond = async (choice: string) => {
    const submission = buildCompanionChoiceSubmission(choice);
    await submitPayload(submission.payload, submission.label);
  };

  const handleSubmitCustom = async () => {
    const text = customInput.trim();
    if (!text) return;
    await handleRespond(text);
  };

  const detailLines = buildCompanionRequestDetailLines(request, responded);

  const bodyExtra = (
    <div className="space-y-2.5">
      {responded ? (
        <div className="flex items-center gap-2 rounded-[8px] border border-success/30 bg-success/10 px-3 py-2 text-sm text-success">
          <span>已回应：{responded}</span>
        </div>
      ) : (
        <>
          {request.isCapabilityGrant ? null : request.options.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {request.options.map((option) => (
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
      badgeToken={request.badgeToken}
      badgeClass={request.badgeClass}
      message={request.prompt}
      detailLines={detailLines}
      bodyExtra={bodyExtra}
      debugChips={request.debugChips}
    />
  );
}
