import Lightbox from "yet-another-react-lightbox";
import Zoom from "yet-another-react-lightbox/plugins/zoom";
import "yet-another-react-lightbox/styles.css";

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
  return (
    <Lightbox
      open={open}
      close={onClose}
      slides={[{ src, alt }]}
      plugins={[Zoom]}
      carousel={{ finite: true, imageFit: "contain" }}
      controller={{
        closeOnBackdropClick: true,
        disableSwipeNavigation: true,
      }}
      labels={{
        Close: "关闭",
        Lightbox: title,
        "Zoom in": "放大",
        "Zoom out": "缩小",
      }}
      render={{
        buttonPrev: () => null,
        buttonNext: () => null,
        slideHeader: () => (
          <div className="pointer-events-none absolute left-3 right-24 top-3 z-10 min-w-0 drop-shadow">
            <div className="truncate font-mono text-xs text-white">{title}</div>
            {subtitle && (
              <div className="mt-0.5 text-[11px] text-white/70">{subtitle}</div>
            )}
          </div>
        ),
      }}
      zoom={{
        scrollToZoom: true,
        wheelZoomDistanceFactor: 220,
        zoomInMultiplier: 1.5,
        doubleClickMaxStops: 2,
        maxZoomPixelRatio: 2.5,
      }}
      animation={{
        fade: 120,
        swipe: 0,
        zoom: 160,
      }}
    />
  );
}
