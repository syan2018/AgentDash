/**
 * <Notice> — Assets Panel 共享反馈条。
 *
 * - 4s auto-dismiss + 右上角 × 关闭
 * - tone: success / danger（视觉走 @agentdash/ui Notice）
 * - notice 为 null 时不渲染
 */

import { useEffect } from "react";

import { Notice as UiNotice, type NoticeTone as UiNoticeTone } from "@agentdash/ui";

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

const TONE_TO_UI: Record<NoticeTone, UiNoticeTone> = {
  success: "success",
  danger: "danger",
};

export function Notice({ notice, onDismiss, autoHideMs = 4000 }: NoticeProps) {
  useEffect(() => {
    if (!notice || autoHideMs <= 0) return;
    const timer = setTimeout(onDismiss, autoHideMs);
    return () => clearTimeout(timer);
  }, [notice, autoHideMs, onDismiss]);

  if (!notice) return null;

  return (
    <UiNotice
      tone={TONE_TO_UI[notice.tone]}
      role={notice.tone === "danger" ? "alert" : "status"}
      className="flex items-center justify-between text-xs"
    >
      <p>{notice.message}</p>
      <button
        type="button"
        onClick={onDismiss}
        className="ml-2 text-xs opacity-70 transition-opacity hover:opacity-100"
        aria-label="关闭提示"
      >
        ×
      </button>
    </UiNotice>
  );
}

export default Notice;
