/**
 * ACP 消息卡片
 *
 * 显示用户消息、Agent 消息和思考过程。
 * 使用 react-markdown + remark-gfm 实现正确的 Markdown 渲染。
 */

import { useState, memo } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

export interface AcpMessageCardProps {
  type: "user" | "agent" | "thinking";
  content: string;
  isStreaming?: boolean;
  collapsible?: boolean;
  defaultCollapsed?: boolean;
}

export const AcpMessageCard = memo(function AcpMessageCard({
  type,
  content,
  isStreaming,
  collapsible = false,
  defaultCollapsed = false,
}: AcpMessageCardProps) {
  const [isCollapsed, setIsCollapsed] = useState(defaultCollapsed);

  const config = MESSAGE_CONFIG[type];

  if (type === "thinking" && !collapsible) {
    return (
      <div className="rounded-lg border border-border/50 bg-muted/20 px-3 py-2.5">
        <button
          type="button"
          onClick={() => setIsCollapsed(!isCollapsed)}
          className="flex w-full items-center justify-between text-sm text-muted-foreground"
        >
          <span className="flex items-center gap-2">
            <span className="text-xs opacity-70">{config.icon}</span>
            <span className="text-xs">思考过程</span>
          </span>
          <span className="text-xs">{isCollapsed ? "展开" : "收起"}</span>
        </button>
        {!isCollapsed && (
          <div className="mt-2 text-sm text-muted-foreground/80">
            <pre className="whitespace-pre-wrap font-mono text-xs leading-relaxed">
              {content}
            </pre>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className={`flex gap-3 ${config.containerClass}`}>
      {/* 头像/图标 */}
      <div
        className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-full ${config.avatarClass}`}
      >
        <span className="text-xs">{config.icon}</span>
      </div>

      {/* 内容 */}
      <div className="flex-1 min-w-0">
        <p className={`mb-1 text-xs ${config.labelClass}`}>{config.label}</p>

        <div className="relative">
          <div className={config.contentClass}>
            {type === "user" ? (
              <p className="whitespace-pre-wrap text-sm leading-relaxed">{content}</p>
            ) : (
              <MarkdownRenderer content={content} />
            )}
          </div>

          {isStreaming && (
            <span className="ml-0.5 inline-block h-4 w-[3px] animate-pulse rounded-sm bg-primary align-text-bottom" />
          )}
        </div>
      </div>
    </div>
  );
});

const MESSAGE_CONFIG = {
  user: {
    icon: "👤",
    label: "用户",
    containerClass: "flex-row",
    avatarClass: "bg-primary/10",
    labelClass: "text-primary font-medium",
    contentClass: "text-foreground",
  },
  agent: {
    icon: "🤖",
    label: "Agent",
    containerClass: "flex-row",
    avatarClass: "bg-success/10",
    labelClass: "text-success font-medium",
    contentClass: "text-foreground",
  },
  thinking: {
    icon: "🧠",
    label: "思考",
    containerClass: "flex-row opacity-70",
    avatarClass: "bg-muted",
    labelClass: "text-muted-foreground",
    contentClass: "text-muted-foreground",
  },
};

const MarkdownRenderer = memo(function MarkdownRenderer({ content }: { content: string }) {
  return (
    <div className="prose prose-sm max-w-none dark:prose-invert prose-p:my-1.5 prose-li:my-0.5 prose-headings:my-2 prose-pre:my-2 prose-code:before:content-none prose-code:after:content-none">
    <Markdown
      remarkPlugins={[remarkGfm]}
      components={{
        pre({ children }) {
          return (
            <pre className="overflow-auto rounded-md bg-muted/60 p-3 text-xs leading-relaxed">
              {children}
            </pre>
          );
        },
        code({ children, className }) {
          const isBlock = className?.startsWith("language-");
          if (isBlock) {
            return <code className="font-mono text-xs">{children}</code>;
          }
          return (
            <code className="rounded bg-muted px-1 py-0.5 text-xs font-mono">
              {children}
            </code>
          );
        },
        table({ children }) {
          return (
            <div className="overflow-auto rounded-md border border-border my-2">
              <table className="min-w-full text-sm">{children}</table>
            </div>
          );
        },
        th({ children }) {
          return (
            <th className="border-b border-border bg-muted/50 px-3 py-1.5 text-left text-xs font-medium">
              {children}
            </th>
          );
        },
        td({ children }) {
          return (
            <td className="border-b border-border px-3 py-1.5 text-xs">
              {children}
            </td>
          );
        },
        a({ children, href }) {
          return (
            <a href={href} className="text-primary hover:underline" target="_blank" rel="noopener noreferrer">
              {children}
            </a>
          );
        },
      }}
    >
      {content}
    </Markdown>
    </div>
  );
});

export default AcpMessageCard;
