/**
 * ContextFrame 单帧卡片入口
 *
 * 现在统一委托给 `ContextFrameStream`：单帧也走"shell + tab + body"结构，
 * 与多帧聚合保持视觉一致。本组件只接收 model 层已解析的 `ContextFrame`。
 */

import type { ContextFrame } from "../model/contextFrame";
import { ContextFrameStream } from "./ContextFrameStream";

export interface ContextFrameCardProps {
  frame: ContextFrame;
  /** 测试或持久化场景：默认展开 shell 以便对其内层做断言 */
  defaultExpanded?: boolean;
}

export function ContextFrameCard({ frame, defaultExpanded }: ContextFrameCardProps) {
  return <ContextFrameStream frames={[frame]} defaultExpanded={defaultExpanded} />;
}

export default ContextFrameCard;
