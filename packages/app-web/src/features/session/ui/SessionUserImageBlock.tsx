/**
 * 用户消息图片专有 block
 *
 * 把用户在 composer 里粘贴 / 拖拽 / 选择的图片（image 块，url 通常为 data URL）
 * 渲染为缩略图网格，点击可放大查看（复用 ZoomableImageLightbox）。
 * 与 ContentBlockCard 的 image 视觉保持一致，归属用户气泡。
 */

import { memo, useState } from "react";
import { ZoomableImageLightbox } from "../../../components/media/zoomable-image-lightbox";
import type { UserMessageImage } from "../model/types";

export interface SessionUserImageBlockProps {
  images: UserMessageImage[];
}

export const SessionUserImageBlock = memo(function SessionUserImageBlock({
  images,
}: SessionUserImageBlockProps) {
  const [activeIndex, setActiveIndex] = useState<number | null>(null);

  if (images.length === 0) return null;

  const active = activeIndex != null ? images[activeIndex] : null;

  return (
    <>
      <div className="flex flex-wrap gap-2">
        {images.map((image, index) => (
          <button
            key={`${index}:${image.url.slice(0, 24)}`}
            type="button"
            onClick={() => setActiveIndex(index)}
            aria-label={`查看${image.alt}`}
            className="group relative h-28 w-28 shrink-0 cursor-zoom-in overflow-hidden rounded-[8px] border border-border bg-secondary/40 transition-colors hover:border-primary/50 focus:outline-none focus:ring-2 focus:ring-primary/40"
          >
            <img
              src={image.url}
              alt={image.alt}
              className="h-full w-full object-cover transition-transform group-hover:scale-[1.03]"
              draggable={false}
            />
          </button>
        ))}
      </div>

      <ZoomableImageLightbox
        open={active != null}
        src={active?.url ?? ""}
        alt={active?.alt ?? ""}
        title={active?.alt ?? "用户图片"}
        onClose={() => setActiveIndex(null)}
      />
    </>
  );
});

export default SessionUserImageBlock;
