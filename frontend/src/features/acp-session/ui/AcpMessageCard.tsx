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

function toFileUri(relPath: string): string {
  const normalized = relPath.replace(/\\/g, "/").replace(/^\/+/, "");
  return `file:///${normalized}`;
}

function getFileIcon(relPath: string): string {
  const ext = relPath.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "ts":
    case "tsx":
    case "js":
    case "jsx":
      return "📜";
    case "json":
    case "jsonc":
      return "📋";
    case "md":
    case "mdx":
      return "📝";
    case "css":
    case "scss":
    case "less":
      return "🎨";
    case "html":
    case "htm":
      return "🌐";
    case "yml":
    case "yaml":
    case "toml":
      return "⚙️";
    case "rs":
      return "🦀";
    case "py":
      return "🐍";
    case "go":
      return "🐹";
    case "java":
      return "☕";
    case "sql":
      return "🗄️";
    case "sh":
    case "bash":
    case "zsh":
      return "🐚";
    case "dockerfile":
      return "🐳";
    default:
      return "📄";
  }
}

const FILE_PILL_CLASS =
  "inline-flex select-none items-center gap-1 rounded-[6px] border border-[#5865F2]/20 bg-[#5865F2]/10 px-2 py-0.5 text-xs font-medium text-[#5865F2] shadow-sm transition-colors hover:bg-[#5865F2]/15 dark:border-[#5865F2]/30 dark:bg-[#5865F2]/20 dark:text-[#00A8FC]";

function renderTextWithFilePills(text: string): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  const re = /<file:([^>]+)>/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = re.exec(text)) !== null) {
    const full = match[0];
    const path = match[1] ?? "";

    if (match.index > lastIndex) {
      nodes.push(text.slice(lastIndex, match.index));
    }

    const fileName = path.replace(/\\/g, "/").split("/").pop() || path;
    nodes.push(
      <span
        key={`${match.index}:${path}`}
        className={FILE_PILL_CLASS}
        title={toFileUri(path)}
        data-file-ref={path}
      >
        <span>{getFileIcon(path)}</span>
        <span className="underline decoration-[#5865F2]/40 underline-offset-2 dark:decoration-[#00A8FC]/40">
          {fileName}
        </span>
      </span>,
    );

    lastIndex = match.index + full.length;
  }

  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex));
  }

  return nodes;
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
              <p className="whitespace-pre-wrap text-sm leading-relaxed">
                {renderTextWithFilePills(content)}
              </p>
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
