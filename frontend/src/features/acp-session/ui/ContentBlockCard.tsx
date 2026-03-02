/**
 * ContentBlock 卡片组件
 *
 * 优雅地展示 resource/resource_link 类型的 ContentBlock。
 * 包含文件类型图标、文件名、路径和预览信息。
 */

import { memo } from "react";
import type { ContentBlock } from "../model/types";

export interface ContentBlockCardProps {
  block: ContentBlock;
  variant?: "compact" | "default";
}

/**
 * 根据 mimeType 获取对应的图标
 */
function getMimeTypeIcon(mimeType?: string): string {
  if (!mimeType) return "📄";

  if (mimeType.startsWith("image/")) return "🖼️";
  if (mimeType.startsWith("audio/")) return "🔊";
  if (mimeType.startsWith("video/")) return "🎬";
  if (mimeType.startsWith("text/")) {
    if (mimeType.includes("markdown")) return "📝";
    if (mimeType.includes("html")) return "🌐";
    if (mimeType.includes("css")) return "🎨";
    if (mimeType.includes("javascript") || mimeType.includes("typescript")) return "📜";
    return "📄";
  }
  if (mimeType.includes("json")) return "📋";
  if (mimeType.includes("pdf")) return "📕";
  if (mimeType.includes("zip") || mimeType.includes("compressed")) return "📦";
  if (mimeType.includes("sql")) return "🗄️";
  if (mimeType.includes("xml")) return "📰";

  return "📄";
}

/**
 * 根据文件扩展名获取图标（作为 mimeType 的备用）
 */
function getFileIconFromName(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase();

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

/**
 * 格式化文件大小
 */
function formatFileSize(bytes?: number | null): string {
  if (bytes == null) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * 从 URI 中提取文件名
 */
function getFileNameFromUri(uri: string): string {
  const normalized = uri.replace(/\\/g, "/");
  const parts = normalized.split("/");
  return parts[parts.length - 1] || uri;
}

/**
 * 获取文件名的颜色类（基于扩展名）
 */
function getFileNameColorClass(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase();

  const colorMap: Record<string, string> = {
    ts: "text-blue-600 dark:text-blue-400",
    tsx: "text-blue-600 dark:text-blue-400",
    js: "text-yellow-600 dark:text-yellow-400",
    jsx: "text-yellow-600 dark:text-yellow-400",
    json: "text-gray-600 dark:text-gray-400",
    md: "text-purple-600 dark:text-purple-400",
    css: "text-pink-600 dark:text-pink-400",
    scss: "text-pink-600 dark:text-pink-400",
    html: "text-orange-600 dark:text-orange-400",
    rs: "text-orange-700 dark:text-orange-500",
    py: "text-green-600 dark:text-green-400",
    go: "text-cyan-600 dark:text-cyan-400",
  };

  return colorMap[ext || ""] || "text-foreground";
}

/**
 * 提取路径（不含文件名）
 */
function getDirectoryPath(uri: string, fileName: string): string {
  const normalized = uri.replace(/\\/g, "/");
  if (normalized.endsWith(fileName)) {
    const dir = normalized.slice(0, -fileName.length);
    return dir.replace(/^file:\/\//, "").replace(/\/$/, "");
  }
  return uri.replace(/^file:\/\//, "");
}

export const ContentBlockCard = memo(function ContentBlockCard({
  block,
  variant = "default",
}: ContentBlockCardProps) {
  if (block.type === "text") {
    return (
      <div className="rounded-lg border border-border/60 bg-card px-4 py-3 shadow-sm">
        <p className="whitespace-pre-wrap text-sm leading-relaxed text-foreground">
          {block.text}
        </p>
      </div>
    );
  }

  if (block.type === "resource_link") {
    const fileName = block.name || getFileNameFromUri(block.uri);
    const icon = getFileIconFromName(fileName);
    const dirPath = getDirectoryPath(block.uri, fileName);
    const sizeText = formatFileSize(block.size);

    return (
      <div className="group relative flex items-center gap-3 rounded-lg border border-border/60 bg-card p-3 shadow-sm transition-all hover:border-primary/30 hover:shadow-md hover:bg-accent/30">
        {/* 图标容器 */}
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-secondary text-xl shadow-inner">
          {icon}
        </div>

        {/* 内容 */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className={`truncate text-sm font-medium ${getFileNameColorClass(fileName)}`}>
              {fileName}
            </span>
            {sizeText && (
              <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
                {sizeText}
              </span>
            )}
          </div>
          <div className="mt-0.5 flex items-center gap-1.5">
            <span className="text-[10px] text-muted-foreground/70">📎 资源链接</span>
            {dirPath && (
              <>
                <span className="text-muted-foreground/40">·</span>
                <span className="truncate text-[10px] text-muted-foreground/60 font-mono">
                  {dirPath}
                </span>
              </>
            )}
          </div>
        </div>

        {/* 悬停装饰 */}
        <div className="absolute inset-0 rounded-lg ring-1 ring-inset ring-primary/0 transition-all group-hover:ring-primary/10" />
      </div>
    );
  }

  if (block.type === "resource") {
    const resource = block.resource;
    const fileName = getFileNameFromUri(resource.uri);
    const icon = resource.mimeType
      ? getMimeTypeIcon(resource.mimeType)
      : getFileIconFromName(fileName);
    const dirPath = getDirectoryPath(resource.uri, fileName);

    // 判断是否有文本内容
    const hasTextContent = "text" in resource && resource.text != null;
    const textLength = hasTextContent ? resource.text!.length : 0;

    if (variant === "compact" || !hasTextContent) {
      return (
        <div className="group relative flex items-center gap-3 rounded-lg border border-border/60 bg-card p-3 shadow-sm transition-all hover:border-primary/30 hover:shadow-md hover:bg-accent/30">
          {/* 图标容器 */}
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-secondary to-secondary/70 text-xl shadow-inner">
            {icon}
          </div>

          {/* 内容 */}
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <span className={`truncate text-sm font-medium ${getFileNameColorClass(fileName)}`}>
                {fileName}
              </span>
              {resource.mimeType && (
                <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  {resource.mimeType.split("/").pop()}
                </span>
              )}
            </div>
            <div className="mt-0.5 flex items-center gap-1.5">
              <span className="text-[10px] text-muted-foreground/70">
                📄 已引用 {textLength > 0 && `(${textLength.toLocaleString()} 字符)`}
              </span>
              {dirPath && (
                <>
                  <span className="text-muted-foreground/40">·</span>
                  <span className="truncate text-[10px] text-muted-foreground/60 font-mono">
                    {dirPath}
                  </span>
                </>
              )}
            </div>
          </div>

          {/* 悬停装饰 */}
          <div className="absolute inset-0 rounded-lg ring-1 ring-inset ring-primary/0 transition-all group-hover:ring-primary/10" />
        </div>
      );
    }

    // 完整版本：带内容预览
    return (
      <div className="group overflow-hidden rounded-lg border border-border/60 bg-card shadow-sm transition-all hover:border-primary/30 hover:shadow-md">
        {/* 头部 */}
        <div className="flex items-center gap-3 border-b border-border/40 bg-gradient-to-r from-secondary/50 to-transparent p-3">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-secondary to-secondary/70 text-xl shadow-inner">
            {icon}
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <span className={`truncate text-sm font-medium ${getFileNameColorClass(fileName)}`}>
                {fileName}
              </span>
              {resource.mimeType && (
                <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  {resource.mimeType.split("/").pop()}
                </span>
              )}
            </div>
            {dirPath && (
              <span className="mt-0.5 block truncate text-[10px] text-muted-foreground/60 font-mono">
                {dirPath}
              </span>
            )}
          </div>
        </div>

        {/* 内容预览 */}
        {hasTextContent && (
          <div className="relative">
            <pre className="max-h-32 overflow-auto bg-muted/30 p-3 text-xs leading-relaxed text-muted-foreground">
              <code>{resource.text}</code>
            </pre>
            {/* 渐变遮罩（当内容溢出时） */}
            <div className="pointer-events-none absolute bottom-0 left-0 right-0 h-6 bg-gradient-to-t from-card to-transparent" />
          </div>
        )}

        {/* 底部信息 */}
        <div className="flex items-center justify-between border-t border-border/40 bg-secondary/20 px-3 py-1.5">
          <span className="text-[10px] text-muted-foreground">
            已引用 · {textLength.toLocaleString()} 字符
          </span>
          <span className="text-[10px] text-muted-foreground/60 font-mono truncate max-w-[200px]">
            {resource.uri}
          </span>
        </div>
      </div>
    );
  }

  if (block.type === "image") {
    return (
      <div className="group overflow-hidden rounded-lg border border-border/60 bg-card shadow-sm transition-all hover:border-primary/30 hover:shadow-md">
        <div className="flex items-center gap-2 border-b border-border/40 bg-secondary/30 px-3 py-2">
          <span className="text-lg">🖼️</span>
          <span className="text-xs text-muted-foreground">
            图片 {block.mimeType && `(${block.mimeType})`}
          </span>
        </div>
        <div className="p-3">
          <img
            src={`data:${block.mimeType};base64,${block.data}`}
            alt="嵌入图片"
            className="max-h-48 max-w-full rounded-md object-contain"
          />
        </div>
      </div>
    );
  }

  return null;
});

export default ContentBlockCard;
