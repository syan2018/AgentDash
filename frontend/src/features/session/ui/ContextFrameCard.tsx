/**
 * ContextFrame 单帧卡片入口
 *
 * 现在统一委托给 `ContextFrameStream`：单帧也走"shell + tab + body"结构，
 * 与多帧聚合保持视觉一致。本组件只是兼容层，负责把 raw `Record<string, unknown>`
 * 解析成 `ContextFrame` 后交给 Stream。
 */

import { parseContextFrame } from "../model/contextFrame";
import { ContextFrameStream } from "./ContextFrameStream";

export interface ContextFrameCardProps {
  data: Record<string, unknown>;
  /** 测试或持久化场景：默认展开 shell 以便对其内层做断言 */
  defaultExpanded?: boolean;
}

export function ContextFrameCard({ data, defaultExpanded }: ContextFrameCardProps) {
  const frame = parseContextFrame(data);
  if (!frame) return null;
  return <ContextFrameStream frames={[frame]} defaultExpanded={defaultExpanded} />;
}

export default ContextFrameCard;
