/**
 * <Notice> — Assets Panel 共享反馈条。
 *
 * - 4s auto-dismiss + 右上角 × 关闭
 * - tone: success (emerald) / danger (destructive)
 * - notice 为 null 时不渲染
 */

import { useEffect } from "react";

export type NoticeTone = "success" | "danger";

export interface NoticeData {
  tone: NoticeTone;
  message: string;
}

export interface NoticeProps {
  notice: NoticeData | null;
  onDismiss: () => void;
  /** 0 表示不自动消失；默认 4000ms */
  autoHideMs?: number;
}

const TONE_CLASSES: Record<NoticeTone, string> = {
  success: "border-emerald-300/30 bg-emerald-500/5 text-emerald-600",
  danger: "border-destructive/30 bg-destructive/5 text-destructive",
};

export function Notice({ notice, onDismiss, autoHideMs = 4000 }: NoticeProps) {
  useEffect(() => {
    if (!notice || autoHideMs <= 0) return;
    const timer = setTimeout(onDismiss, autoHideMs);
    return () => clearTimeout(timer);
  }, [notice, autoHideMs, onDismiss]);

  if (!notice) return null;

  return (
    <div
      className={`flex items-center justify-between rounded-[8px] border px-3 py-2 ${TONE_CLASSES[notice.tone]}`}
      role={notice.tone === "danger" ? "alert" : "status"}
    >
      <p className="text-xs">{notice.message}</p>
      <button
        type="button"
        onClick={onDismiss}
        className="ml-2 text-xs opacity-70 transition-opacity hover:opacity-100"
        aria-label="关闭提示"
      >
        ×
      </button>
    </div>
  );
}

export default Notice;
