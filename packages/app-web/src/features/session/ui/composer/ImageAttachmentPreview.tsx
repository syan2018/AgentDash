/**
 * 图片附件缩略图预览行
 *
 * 展示已添加的图片缩略图 + 单张删除按钮。空时不渲染。
 */

import type { ImageAttachment } from "./useImageAttachments";

interface ImageAttachmentPreviewProps {
  attachments: ImageAttachment[];
  onRemove: (id: string) => void;
}

export function ImageAttachmentPreview({ attachments, onRemove }: ImageAttachmentPreviewProps) {
  if (attachments.length === 0) return null;

  return (
    <div className="flex flex-wrap gap-2 px-1 pb-2">
      {attachments.map((att) => (
        <div
          key={att.id}
          className="group relative h-16 w-16 shrink-0 overflow-hidden rounded-[8px] border border-border bg-secondary"
        >
          <img
            src={att.previewUrl}
            alt={att.file.name}
            className="h-full w-full object-cover"
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
  );
}
