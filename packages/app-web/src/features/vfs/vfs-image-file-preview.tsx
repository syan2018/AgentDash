import { useState } from "react";
import { ZoomableImageLightbox } from "../../components/media/zoomable-image-lightbox";
import { formatBytes } from "./vfs-format";

export interface VfsImageFilePreviewProps {
  path: string;
  src: string;
  mimeType?: string | null;
  size?: number | null;
}

export function VfsImageFilePreview({ path, src, mimeType, size }: VfsImageFilePreviewProps) {
  const [lightboxPath, setLightboxPath] = useState<string | null>(null);
  const subtitle = `${mimeType ?? "image/*"} · ${size != null ? formatBytes(size) : "未知大小"}`;
  const lightboxOpen = lightboxPath === path;

  return (
    <div className="flex h-full flex-col overflow-hidden bg-background">
      <div className="shrink-0 border-b border-border px-3 py-2">
        <div className="truncate font-mono text-xs text-foreground">{path}</div>
        <div className="mt-0.5 text-[11px] text-muted-foreground">{subtitle}</div>
      </div>
      <div
        role="button"
        tabIndex={0}
        aria-label="展开图片"
        onDoubleClick={() => setLightboxPath(path)}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            setLightboxPath(path);
          }
        }}
        className="flex min-h-0 flex-1 cursor-zoom-in items-center justify-center overflow-auto bg-secondary/10 p-4 focus:outline-none focus:ring-2 focus:ring-primary/40 focus:ring-inset"
      >
        <img
          src={src}
          alt={path}
          className="max-h-full max-w-full rounded-[8px] border border-border bg-background object-contain"
          draggable={false}
        />
      </div>
      <ZoomableImageLightbox
        open={lightboxOpen}
        src={src}
        alt={path}
        title={path}
        subtitle={subtitle}
        onClose={() => setLightboxPath(null)}
      />
    </div>
  );
}
