/**
 * 通用 Markdown 渲染器
 *
 * 复用会话消息已有的 Streamdown 插件组合，保证聊天内容与资产预览的
 * Markdown 展示能力一致。
 */

import { memo } from "react";
import { Streamdown } from "streamdown";
import { code } from "@streamdown/code";
import { math } from "@streamdown/math";
import { mermaid } from "@streamdown/mermaid";
import { cjk } from "@streamdown/cjk";

export interface MarkdownRendererProps {
  content: string;
  isStreaming?: boolean;
  className?: string;
}

export const MarkdownRenderer = memo(function MarkdownRenderer({
  content,
  isStreaming = false,
  className = "",
}: MarkdownRendererProps) {
  return (
    <div className={`agentdash-markdown ${className}`}>
      <Streamdown isAnimating={isStreaming} plugins={{ code, math, mermaid, cjk }}>
        {content}
      </Streamdown>
    </div>
  );
});

export default MarkdownRenderer;
