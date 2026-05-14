/**
 * ContentBlock 卡片组件
 *
 * 用统一的会话卡片视觉展示 resource/resource_link 类型的内容块。
 */

import { memo } from "react";
import type { ContentBlock } from "../model/types";

export interface ContentBlockCardProps {
  block: ContentBlock;
  variant?: "compact" | "default";
}

function getMimeTypeBadge(mimeType?: string): string {
  if (!mimeType) return "FILE";

  if (mimeType.startsWith("image/")) return "IMAGE";
  if (mimeType.startsWith("audio/")) return "AUDIO";
  if (mimeType.startsWith("video/")) return "VIDEO";
  if (mimeType.startsWith("text/")) {
    if (mimeType.includes("markdown")) return "MD";
    if (mimeType.includes("html")) return "HTML";
    if (mimeType.includes("css")) return "STYLE";
    if (mimeType.includes("javascript") || mimeType.includes("typescript")) return "CODE";
    return "TEXT";
  }
  if (mimeType.includes("json")) return "JSON";
  if (mimeType.includes("pdf")) return "PDF";
  if (mimeType.includes("zip") || mimeType.includes("compressed")) return "ARCHIVE";
  if (mimeType.includes("sql")) return "SQL";
  if (mimeType.includes("xml")) return "XML";

  return "FILE";
}

function getFileBadgeFromName(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase();

  switch (ext) {
    case "ts":
    case "tsx":
    case "js":
    case "jsx":
    case "rs":
    case "py":
    case "go":
    case "java":
      return "CODE";
    case "json":
    case "jsonc":
      return "JSON";
    case "md":
    case "mdx":
      return "MD";
    case "css":
    case "scss":
    case "less":
      return "STYLE";
    case "html":
    case "htm":
      return "HTML";
    case "yml":
    case "yaml":
    case "toml":
      return "CFG";
    case "sql":
      return "SQL";
    case "sh":
    case "bash":
    case "zsh":
      return "SHELL";
    case "dockerfile":
      return "DOCKER";
    default:
      return "FILE";
  }
}

function formatFileSize(bytes?: number | null): string {
  if (bytes == null) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function getFileNameFromUri(uri: string): string {
  const normalized = uri.replace(/\\/g, "/");
  const parts = normalized.split("/");
  return parts[parts.length - 1] || uri;
}

function getDirectoryPath(uri: string, fileName: string): string {
  const normalized = uri.replace(/\\/g, "/");
  if (normalized.endsWith(fileName)) {
    const dir = normalized.slice(0, -fileName.length);
    return dir.replace(/^file:\/\//, "").replace(/\/$/, "");
  }
  return uri.replace(/^file:\/\//, "");
}

function ResourceHeader({
  badge,
  title,
  meta,
  path,
}: {
  badge: string;
  title: string;
  meta?: string | null;
  path?: string | null;
}) {
  return (
    <div className="flex items-start gap-3">
      <span className="inline-flex min-w-12 shrink-0 items-center justify-center rounded-[8px] border border-border bg-secondary px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
        {badge}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="truncate text-sm font-medium text-foreground">{title}</span>
          {meta && (
            <span className="shrink-0 rounded-[6px] border border-border bg-secondary/60 px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {meta}
            </span>
          )}
        </div>
        {path && (
          <span className="mt-1 block truncate font-mono text-[10px] text-muted-foreground/70">
            {path}
          </span>
        )}
      </div>
    </div>
  );
}

export const ContentBlockCard = memo(function ContentBlockCard({
  block,
  variant = "default",
}: ContentBlockCardProps) {
  if (block.type === "text") {
    return (
      <div className="rounded-[12px] border border-border bg-background px-4 py-3">
        <p className="whitespace-pre-wrap text-sm leading-relaxed text-foreground">
          {block.text}
        </p>
      </div>
    );
  }

  if (block.type === "resource_link") {
    const fileName = block.name || getFileNameFromUri(block.uri);
    const badge = getFileBadgeFromName(fileName);
    const dirPath = getDirectoryPath(block.uri, fileName);
    const sizeText = formatFileSize(block.size);

    return (
      <div className="rounded-[12px] border border-border bg-background px-3 py-3 transition-colors hover:bg-secondary/20">
        <ResourceHeader badge={badge} title={fileName} path={dirPath} />
        <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
          <span>资源链接</span>
          {sizeText && (
            <>
              <span>·</span>
              <span className="tabular-nums">{sizeText}</span>
            </>
          )}
        </div>
      </div>
    );
  }

  if (block.type === "resource") {
    const resource = block.resource;
    const fileName = getFileNameFromUri(resource.uri);
    const badge = resource.mimeType
      ? getMimeTypeBadge(resource.mimeType)
      : getFileBadgeFromName(fileName);
    const dirPath = getDirectoryPath(resource.uri, fileName);
    const hasTextContent = "text" in resource && resource.text != null;
    const resourceText = hasTextContent ? resource.text : null;
    const textLength = resourceText?.length ?? 0;
    const mimeLabel = resource.mimeType?.split("/").pop() ?? null;

    if (variant === "compact" || !hasTextContent) {
      return (
        <div className="rounded-[12px] border border-border bg-background px-3 py-3 transition-colors hover:bg-secondary/20">
          <ResourceHeader badge={badge} title={fileName} meta={mimeLabel} path={dirPath} />
          <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
            <span>已引用{textLength > 0 ? ` · ${textLength.toLocaleString()} 字符` : ""}</span>
          </div>
        </div>
      );
    }

    return (
      <div className="overflow-hidden rounded-[12px] border border-border bg-background">
        <div className="border-b border-border px-3 py-3">
          <ResourceHeader badge={badge} title={fileName} meta={mimeLabel} path={dirPath} />
        </div>

        {hasTextContent && (
          <div className="relative">
            <pre className="max-h-32 overflow-auto bg-secondary/20 p-3 text-xs leading-relaxed text-foreground/85">
              <code>{resourceText}</code>
            </pre>
            <div className="pointer-events-none absolute bottom-0 left-0 right-0 h-6 bg-gradient-to-t from-background to-transparent" />
          </div>
        )}

        <div className="flex items-center justify-between gap-3 border-t border-border px-3 py-2">
          <span className="text-[10px] text-muted-foreground">
            已引用 · {textLength.toLocaleString()} 字符
          </span>
          <span className="max-w-[240px] truncate font-mono text-[10px] text-muted-foreground/70">
            {resource.uri}
          </span>
        </div>
      </div>
    );
  }

  if (block.type === "image") {
    return (
      <div className="overflow-hidden rounded-[12px] border border-border bg-background">
        <div className="flex items-center gap-2 border-b border-border px-3 py-2.5">
          <span className="inline-flex min-w-12 items-center justify-center rounded-[8px] border border-border bg-secondary px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            IMAGE
          </span>
          <span className="text-xs text-muted-foreground">
            图片 {block.mimeType && `(${block.mimeType})`}
          </span>
        </div>
        <div className="p-3">
          <img
            src={`data:${block.mimeType};base64,${block.data}`}
            alt="嵌入图片"
            className="max-h-48 max-w-full rounded-[10px] border border-border object-contain"
          />
        </div>
      </div>
    );
  }

  return null;
});

export default ContentBlockCard;
