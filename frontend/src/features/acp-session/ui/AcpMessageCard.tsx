/**
 * ACP 消息卡片
 *
 * 显示用户消息、Agent 消息和思考过程
 */

import { useState } from "react";

export interface AcpMessageCardProps {
  /** 消息类型 */
  type: "user" | "agent" | "thinking";
  /** 消息内容 */
  content: string;
  /** 是否流式传输中 */
  isStreaming?: boolean;
  /** 是否可折叠 */
  collapsible?: boolean;
  /** 默认折叠状态 */
  defaultCollapsed?: boolean;
}

export function AcpMessageCard({
  type,
  content,
  isStreaming,
  collapsible = false,
  defaultCollapsed = false,
}: AcpMessageCardProps) {
  const [isCollapsed, setIsCollapsed] = useState(defaultCollapsed);

  const config = MESSAGE_CONFIG[type];

  // 思考消息默认折叠
  if (type === "thinking" && !collapsible) {
    return (
      <div className="rounded-md border border-border bg-muted/30 p-3">
        <button
          type="button"
          onClick={() => setIsCollapsed(!isCollapsed)}
          className="flex w-full items-center justify-between text-sm text-muted-foreground"
        >
          <span className="flex items-center gap-2">
            <span>{config.icon}</span>
            <span>思考过程</span>
          </span>
          <span>{isCollapsed ? "展开" : "收起"}</span>
        </button>
        {!isCollapsed && (
          <div className="mt-2 text-sm text-muted-foreground">
            <pre className="whitespace-pre-wrap font-mono text-xs opacity-70">
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
        className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-full ${config.avatarClass}`}
      >
        <span className="text-sm">{config.icon}</span>
      </div>

      {/* 内容 */}
      <div className="flex-1 min-w-0">
        {/* 标签 */}
        <p className={`mb-1 text-xs ${config.labelClass}`}>{config.label}</p>

        {/* 消息内容 */}
        <div className="relative">
          <div
            className={`prose prose-sm max-w-none dark:prose-invert ${config.contentClass}`}
          >
            {type === "user" ? (
              <p className="whitespace-pre-wrap">{content}</p>
            ) : (
              <MarkdownContent content={content} />
            )}
          </div>

          {/* 流式指示器 */}
          {isStreaming && (
            <span className="ml-1 inline-block h-4 w-2 animate-pulse bg-primary" />
          )}
        </div>
      </div>
    </div>
  );
}

/** 消息配置 */
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

/** Markdown 内容渲染（简化版） */
function MarkdownContent({ content }: { content: string }) {
  // 简单的 Markdown 渲染
  // 实际项目中可以使用 react-markdown 等库
  const lines = content.split("\n");

  return (
    <div className="space-y-2">
      {lines.map((line, index) => {
        // 代码块
        if (line.startsWith("```")) {
          return (
            <pre
              key={index}
              className="overflow-auto rounded-md bg-muted/50 p-3 text-xs"
            >
              <code>{line.replace(/```/g, "")}</code>
            </pre>
          );
        }

        // 行内代码
        if (line.includes("`")) {
          const parts = line.split(/(`[^`]+`)/);
          return (
            <p key={index} className="whitespace-pre-wrap">
              {parts.map((part, i) =>
                part.startsWith("`") && part.endsWith("`") ? (
                  <code
                    key={i}
                    className="rounded bg-muted px-1 py-0.5 text-xs font-mono"
                  >
                    {part.slice(1, -1)}
                  </code>
                ) : (
                  part
                )
              )}
            </p>
          );
        }

        // 列表项
        if (line.startsWith("- ") || line.startsWith("* ")) {
          return (
            <li key={index} className="ml-4">
              {line.slice(2)}
            </li>
          );
        }

        // 标题
        if (line.startsWith("# ")) {
          return (
            <h1 key={index} className="text-lg font-bold">
              {line.slice(2)}
            </h1>
          );
        }
        if (line.startsWith("## ")) {
          return (
            <h2 key={index} className="text-base font-semibold">
              {line.slice(3)}
            </h2>
          );
        }
        if (line.startsWith("### ")) {
          return (
            <h3 key={index} className="text-sm font-medium">
              {line.slice(4)}
            </h3>
          );
        }

        // 普通段落
        if (line.trim()) {
          return (
            <p key={index} className="whitespace-pre-wrap">
              {line}
            </p>
          );
        }

        // 空行
        return <br key={index} />;
      })}
    </div>
  );
}

export default AcpMessageCard;
