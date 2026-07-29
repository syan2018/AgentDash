/**
 * 上下文压缩 lifecycle body — 极简指示
 */

import { CB } from "./cardBodyTokens";

export interface ContextCompactionCardBodyProps {
  status: "inProgress" | "succeeded" | "failed" | "lost" | "cancelled";
  error?: string | null;
}

export function ContextCompactionCardBody({
  status,
  error,
}: ContextCompactionCardBodyProps) {
  const message = error ?? {
    inProgress: "正在压缩上下文…",
    succeeded: "上下文已压缩，降低后续 token 用量。",
    failed: "上下文压缩失败。",
    lost: "上下文压缩状态丢失，需要恢复后再继续。",
    cancelled: "上下文压缩已取消。",
  }[status];

  return (
    <p className={CB.meta}>
      {message}
    </p>
  );
}
