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
import { ST } from "./bodies/cardBodyTokens";

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
  defaultCollapsed,
  badgeOverride: _badgeOverride,
  labelOverride: _labelOverride,
  images,
}: SessionMessageCardProps) {
  const [isCollapsed, setIsCollapsed] = useState(defaultCollapsed ?? type === "thinking");
  const hasImages = type === "user" && Boolean(images && images.length > 0);
  const hasText = content.trim().length > 0;

  if (type === "thinking") {
    const canExpand = hasText;
    const label = isStreaming ? "正在思考" : "思考";
    return (
      <div>
        <button
          type="button"
          onClick={() => {
            if (canExpand) setIsCollapsed(!isCollapsed);
          }}
          className={ST.groupRow}
        >
          <span className={ST.chevron}>{canExpand ? (isCollapsed ? "▶" : "▼") : "•"}</span>
          <span className={ST.badge}>THINK</span>
          <span className={ST.hint}>{label}</span>
        </button>

        {canExpand && !isCollapsed && (
          <div className={ST.itemList}>
            <div className={ST.bodyArea}>
              <pre className="whitespace-pre-wrap text-xs leading-6 text-muted-foreground/75">{content}</pre>
            </div>
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
