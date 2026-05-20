import { useEffect } from "react";
import { TransformComponent, TransformWrapper } from "react-zoom-pan-pinch";
import type { ReactZoomPanPinchContentRef } from "react-zoom-pan-pinch";

export interface ZoomableImageLightboxProps {
  open: boolean;
  src: string;
  alt: string;
  title: string;
  subtitle?: string;
  onClose: () => void;
}

export function ZoomableImageLightbox({
  open,
  src,
  alt,
  title,
  subtitle,
  onClose,
}: ZoomableImageLightboxProps) {
  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose, open]);

  if (!open) return null;

  return (
    <>
      <div className="fixed inset-0 z-[92] bg-foreground/24 backdrop-blur-[2px]" onClick={onClose} />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onClick={onClose}
        className="fixed inset-0 z-[93] flex items-center justify-center p-4 sm:p-6"
      >
        <div
          className="flex h-full max-h-[calc(100vh-2rem)] w-full max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-[8px] border border-border bg-background shadow-2xl sm:max-h-[calc(100vh-3rem)] sm:max-w-[calc(100vw-3rem)]"
          onClick={(event) => event.stopPropagation()}
        >
          <TransformWrapper
            key={src}
            initialScale={1}
            minScale={0.2}
            maxScale={6}
            centerOnInit
            centerZoomedOut
            limitToBounds={false}
            wheel={{ step: 0.04 }}
            doubleClick={{ mode: "toggle", step: 1.2 }}
          >
            {({ zoomIn, zoomOut, resetTransform, centerView }: ReactZoomPanPinchContentRef) => (
              <>
                <div className="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-mono text-xs text-foreground">{title}</div>
                    {subtitle && (
                      <div className="mt-0.5 text-[11px] text-muted-foreground">{subtitle}</div>
                    )}
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <LightboxToolButton title="放大" onClick={() => zoomIn(0.4)}>
                      +
                    </LightboxToolButton>
                    <LightboxToolButton title="缩小" onClick={() => zoomOut(0.4)}>
                      -
                    </LightboxToolButton>
                    <LightboxToolButton title="重置缩放" onClick={() => resetTransform()}>
                      1:1
                    </LightboxToolButton>
                    <LightboxToolButton title="居中" onClick={() => centerView()}>
                      ⊙
                    </LightboxToolButton>
                    <LightboxToolButton title="关闭" onClick={onClose}>
                      ×
                    </LightboxToolButton>
                  </div>
                </div>
                <div className="min-h-0 flex-1 bg-secondary/10">
                  <TransformComponent
                    wrapperClass="!h-full !w-full"
                    contentClass="!h-full !w-full"
                  >
                    <div className="flex h-full w-full items-center justify-center p-3 sm:p-4">
                      <img
                        src={src}
                        alt={alt}
                        className="max-h-full max-w-full select-none object-contain"
                        draggable={false}
                      />
                    </div>
                  </TransformComponent>
                </div>
              </>
            )}
          </TransformWrapper>
        </div>
      </div>
    </>
  );
}

interface LightboxToolButtonProps {
  title: string;
  onClick: () => void;
  children: string;
}

function LightboxToolButton({ title, onClick, children }: LightboxToolButtonProps) {
  return (
    <button
      type="button"
      aria-label={title}
      title={title}
      onClick={onClick}
      className="inline-flex h-7 min-w-7 shrink-0 items-center justify-center rounded-[4px] border border-border px-1.5 text-xs leading-none text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
    >
      {children}
    </button>
  );
}
