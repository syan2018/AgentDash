import { memo, useState, type ReactNode } from "react";
import { MarkdownRenderer } from "../../../components/ui/markdown-renderer";
import {
  FILE_PILL_BADGE_CLASS,
  FILE_PILL_CLASS,
  FILE_PILL_LABEL_CLASS,
  getDisplayFileName,
  getFileKindLabel,
  toFileUri,
} from "../../file-reference/fileReferenceUi";
import { SessionUserImageBlock } from "./SessionUserImageBlock";
import type { UserMessageImage } from "../model/types";

export interface SessionMessageCardProps {
  type: "user" | "agent" | "thinking";
  content: string;
  isStreaming?: boolean;
  collapsible?: boolean;
  defaultCollapsed?: boolean;
  badgeOverride?: string;
  labelOverride?: string;
  /** 仅用户消息：随文本一起展示的图片块。 */
  images?: UserMessageImage[];
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

export const SessionMessageCard = memo(function SessionMessageCard({
  type,
  content,
  isStreaming,
  collapsible: _collapsible = false,
  defaultCollapsed = false,
  badgeOverride: _badgeOverride,
  labelOverride: _labelOverride,
  images,
}: SessionMessageCardProps) {
  const [isCollapsed, setIsCollapsed] = useState(defaultCollapsed);
  const hasImages = type === "user" && Boolean(images && images.length > 0);
  const hasText = content.trim().length > 0;

  // ── Thinking：轻量折叠行 ──
  if (type === "thinking") {
    return (
      <div className="group">
        <button
          type="button"
          onClick={() => setIsCollapsed(!isCollapsed)}
          className="flex w-full items-center gap-2 py-1 text-left text-xs text-muted-foreground/70 transition-colors hover:text-muted-foreground"
        >
          <span className="inline-block h-px flex-1 max-w-4 bg-border/60" />
          <span className="shrink-0 font-medium">思考</span>
          <span className="inline-block h-px flex-1 bg-border/60" />
          <span className="shrink-0 text-[10px]">{isCollapsed ? "展开" : "收起"}</span>
        </button>

        {!isCollapsed && (
          <div className="pl-1 pt-1">
            <pre className="whitespace-pre-wrap text-xs leading-6 text-muted-foreground/75">{content}</pre>
          </div>
        )}
      </div>
    );
  }

  // ── User：右侧对齐气泡 ──
  if (type === "user") {
    return (
      <div className="flex justify-end">
        <div className="min-w-0 max-w-[85%] rounded-[12px] rounded-tr-[4px] bg-primary/10 px-4 py-2.5">
          <div className="space-y-2.5">
            {hasText && (
              <p className="whitespace-pre-wrap wrap-anywhere text-sm leading-7 text-foreground">
                {renderTextWithFilePills(content)}
              </p>
            )}
            {hasImages && <SessionUserImageBlock images={images!} />}
          </div>
        </div>
      </div>
    );
  }

  // ── Agent：无边框文档流 ──
  return (
    <div className="py-0.5">
      <MarkdownRenderer content={content} isStreaming={isStreaming} />
      {isStreaming && (
        <span className="mt-1 inline-flex h-4 w-[2px] animate-pulse rounded-[4px] bg-primary align-middle" />
      )}
    </div>
  );
});

export default SessionMessageCard;
