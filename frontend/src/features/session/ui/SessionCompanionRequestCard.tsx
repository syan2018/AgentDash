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
import { extractPlatformEventData } from "../model/agentdashMeta";
import { EventFullCard } from "./EventCards";
import { respondCompanionRequest } from "../../../services/executor";

export interface AcpCompanionRequestCardProps {
  event: BackboneEvent;
  sessionId?: string;
}

export function AcpCompanionRequestCard({ event, sessionId }: AcpCompanionRequestCardProps) {
  const data = extractPlatformEventData(event);

  const requestId = typeof data?.request_id === "string" ? data.request_id : null;
  const prompt = typeof data?.prompt === "string" ? data.prompt : "Agent 请求你回应";
  const options = Array.isArray(data?.options) ? (data.options as string[]) : [];
  const wait = data?.wait === true;

  const [isSubmitting, setIsSubmitting] = useState(false);
  const [responded, setResponded] = useState<string | null>(null);
  const [customInput, setCustomInput] = useState("");
  const [error, setError] = useState<string | null>(null);

  const handleRespond = async (choice: string) => {
    if (!sessionId || !requestId || isSubmitting) return;
    setError(null);
    setIsSubmitting(true);
    try {
      await respondCompanionRequest(sessionId, requestId, {
        type: "decision",
        status: "approved",
        choice,
        summary: choice,
      });
      setResponded(choice);
    } catch (err) {
      setError(err instanceof Error ? err.message : "回应失败");
    } finally {
      setIsSubmitting(false);
    }
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

  const badge = wait
    ? "border-warning/25 bg-warning/10 text-warning"
    : "border-primary/25 bg-primary/8 text-primary";

  const bodyExtra = (
    <div className="space-y-2.5">
      {responded ? (
        <div className="flex items-center gap-2 rounded-[10px] border border-success/30 bg-success/10 px-3 py-2 text-sm text-success">
          <span>已回应：{responded}</span>
        </div>
      ) : (
        <>
          {options.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {options.map((option) => (
                <button
                  key={option}
                  type="button"
                  onClick={() => { void handleRespond(option); }}
                  disabled={isSubmitting}
                  className="rounded-[10px] border border-primary/30 bg-primary/10 px-3 py-1.5 text-sm text-primary transition-colors hover:bg-primary/20 disabled:opacity-50"
                >
                  {isSubmitting ? "处理中…" : option}
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
                className="min-w-0 flex-1 rounded-[10px] border border-border bg-background px-3 py-1.5 text-sm outline-none focus:border-primary/50 disabled:opacity-50"
              />
              <button
                type="button"
                onClick={() => { void handleSubmitCustom(); }}
                disabled={isSubmitting || !customInput.trim()}
                className="rounded-[10px] border border-primary/30 bg-primary/10 px-3 py-1.5 text-sm text-primary transition-colors hover:bg-primary/20 disabled:opacity-50"
              >
                {isSubmitting ? "处理中…" : "提交"}
              </button>
            </div>
          )}
        </>
      )}
      {error && (
        <div className="rounded-[10px] border border-destructive/30 bg-destructive/10 p-2 text-sm text-destructive">
          {error}
        </div>
      )}
    </div>
  );

  return (
    <EventFullCard
      badgeToken={wait ? "等待回应" : "请求"}
      badgeClass={badge}
      message={prompt}
      detailLines={detailLines}
      bodyExtra={bodyExtra}
      debugChips={requestId ? [`request: ${requestId.slice(0, 12)}`] : []}
    />
  );
}
