/**
 * 上下文压缩 lifecycle body — 极简指示
 */

import { CB } from "./cardBodyTokens";

export function ContextCompactionCardBody() {
  return (
    <p className={CB.meta}>
      上下文已压缩，降低后续 token 用量。
    </p>
  );
}
