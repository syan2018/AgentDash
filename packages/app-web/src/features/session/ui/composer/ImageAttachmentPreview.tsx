/**
 * 图片附件缩略图预览行
 *
 * 缩略图可点击查看大图（lightbox 覆盖层），hover 显示删除按钮。
 */

import { useState } from "react";
import type { ImageAttachment } from "./useImageAttachments";

interface ImageAttachmentPreviewProps {
  attachments: ImageAttachment[];
  onRemove: (id: string) => void;
}

export function ImageAttachmentPreview({ attachments, onRemove }: ImageAttachmentPreviewProps) {
  const [viewingUrl, setViewingUrl] = useState<string | null>(null);

  if (attachments.length === 0) return null;

  return (
    <>
      <div className="flex flex-wrap gap-2 pb-2">
        {attachments.map((att) => (
          <div
            key={att.id}
            className="group relative h-16 w-16 shrink-0 overflow-hidden rounded-[8px] border border-border bg-secondary"
          >
            <img
              src={att.previewUrl}
              alt={att.file.name}
              className="h-full w-full cursor-pointer object-cover"
              onClick={() => setViewingUrl(att.previewUrl)}
            />
            <button
              type="button"
              onClick={() => onRemove(att.id)}
              className="absolute -right-1 -top-1 flex h-5 w-5 items-center justify-center rounded-[6px] bg-foreground text-background opacity-0 shadow-sm transition-opacity group-hover:opacity-100"
              title="移除"
            >
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                <path d="M2 2L8 8M8 2L2 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
              </svg>
            </button>
          </div>
        ))}
      </div>

      {/* Lightbox */}
      {viewingUrl && (
        <div
          className="fixed inset-0 z-[100] flex items-center justify-center bg-black/70 backdrop-blur-sm"
          onClick={() => setViewingUrl(null)}
        >
          <img
            src={viewingUrl}
            alt="预览"
            className="max-h-[85vh] max-w-[90vw] rounded-[8px] object-contain shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          />
          <button
            type="button"
            onClick={() => setViewingUrl(null)}
            className="absolute right-4 top-4 flex h-10 w-10 items-center justify-center rounded-[8px] bg-white/20 text-white transition-colors hover:bg-white/40"
          >
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M4 4L16 16M16 4L4 16" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            </svg>
          </button>
        </div>
      )}
    </>
  );
}
