import { memo, useState, type ReactNode } from "react";
import { Streamdown } from "streamdown";
import { code } from "@streamdown/code";
import { math } from "@streamdown/math";
import { mermaid } from "@streamdown/mermaid";
import { cjk } from "@streamdown/cjk";
import {
  FILE_PILL_BADGE_CLASS,
  FILE_PILL_CLASS,
  FILE_PILL_LABEL_CLASS,
  getDisplayFileName,
  getFileKindLabel,
  toFileUri,
} from "../../file-reference/fileReferenceUi";

export interface AcpMessageCardProps {
  type: "user" | "agent" | "thinking";
  content: string;
  isStreaming?: boolean;
  collapsible?: boolean;
  defaultCollapsed?: boolean;
}

function renderTextWithFilePills(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const re = /<file:([^>]+)>/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = re.exec(text)) !== null) {
    const full = match[0];
    const path = match[1] ?? "";

    if (match.index > lastIndex) {
      nodes.push(text.slice(lastIndex, match.index));
    }

    nodes.push(
      <span
        key={`${match.index}:${path}`}
        className={FILE_PILL_CLASS}
        title={toFileUri(path)}
        data-file-ref={path}
      >
        <span className={FILE_PILL_BADGE_CLASS}>{getFileKindLabel(path)}</span>
        <span className={FILE_PILL_LABEL_CLASS}>{getDisplayFileName(path)}</span>
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
      <div className="rounded-[12px] border border-dashed border-border bg-secondary/60 px-4 py-3.5">
        <button
          type="button"
          onClick={() => setIsCollapsed(!isCollapsed)}
          className="flex w-full items-center justify-between gap-3 text-left"
        >
          <span className="flex items-center gap-2">
            <span className="inline-flex h-5 min-w-5 items-center justify-center rounded-[6px] border border-border bg-background px-1.5 text-[10px] font-semibold tracking-[0.14em] text-muted-foreground">
              {config.badge}
            </span>
            <span className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
              思考
            </span>
          </span>
          <span className="text-xs text-muted-foreground">{isCollapsed ? "展开" : "收起"}</span>
        </button>

        {!isCollapsed && (
          <div className="mt-2.5 border-t border-border/70 pt-2.5 text-sm text-muted-foreground">
            <pre className="whitespace-pre-wrap text-xs leading-6">{content}</pre>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className={`flex gap-3 ${config.containerClass}`}>
      <div className="flex w-11 shrink-0 flex-col items-start pt-0.5">
        <span className={config.avatarClass}>{config.badge}</span>
        <span className={config.labelClass}>{config.label}</span>
      </div>

      <div className="min-w-0 flex-1">
        <div className={config.contentClass}>
          {type === "user" ? (
            <p className="whitespace-pre-wrap text-sm leading-7 text-foreground">
              {renderTextWithFilePills(content)}
            </p>
          ) : (
            <MarkdownRenderer content={content} isStreaming={isStreaming} />
          )}

          {isStreaming && (
            <span className="mt-3 inline-flex h-4 w-[2px] animate-pulse rounded-full bg-primary align-middle" />
          )}
        </div>
      </div>
    </div>
  );
});

const MESSAGE_CONFIG = {
  user: {
    badge: "ME",
    label: "用户",
    containerClass: "items-start",
    avatarClass:
      "inline-flex h-6 min-w-6 items-center justify-center rounded-[7px] border border-border bg-secondary px-1.5 text-[10px] font-semibold tracking-[0.14em] text-foreground",
    labelClass: "mt-1 text-[11px] uppercase tracking-[0.14em] text-muted-foreground",
    contentClass: "rounded-[12px] border border-border bg-secondary px-4 py-3.5",
  },
  agent: {
    badge: "AI",
    label: "Agent",
    containerClass: "items-start",
    avatarClass:
      "inline-flex h-6 min-w-6 items-center justify-center rounded-[7px] border border-border bg-background px-1.5 text-[10px] font-semibold tracking-[0.14em] text-foreground",
    labelClass: "mt-1 text-[11px] uppercase tracking-[0.14em] text-muted-foreground",
    contentClass: "rounded-[12px] border border-border bg-background px-4 py-3.5",
  },
  thinking: {
    badge: "TH",
    label: "思考",
    containerClass: "items-start opacity-85",
    avatarClass:
      "inline-flex h-6 min-w-6 items-center justify-center rounded-[7px] border border-border bg-secondary px-1.5 text-[10px] font-semibold tracking-[0.14em] text-muted-foreground",
    labelClass: "mt-1 text-[11px] uppercase tracking-[0.14em] text-muted-foreground",
    contentClass: "rounded-[12px] border border-dashed border-border bg-secondary/60 px-4 py-3.5 text-muted-foreground",
  },
} as const;

const MarkdownRenderer = memo(function MarkdownRenderer({
  content,
  isStreaming,
}: {
  content: string;
  isStreaming?: boolean;
}) {
  return (
    <div className="agentdash-chat-markdown">
      <Streamdown
        isAnimating={isStreaming ?? false}
        plugins={{ code, math, mermaid, cjk }}
      >
        {content}
      </Streamdown>
    </div>
  );
});

export default AcpMessageCard;
