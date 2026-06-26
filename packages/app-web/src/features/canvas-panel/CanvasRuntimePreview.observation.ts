import type { UploadCanvasRenderObservationInput } from "../../services/canvas";

export function buildPreviewFailureObservation(
  frameId: string,
  generation: number,
  message: string,
  viewportElement: Pick<HTMLElement, "clientWidth" | "clientHeight"> | null,
): UploadCanvasRenderObservationInput {
  const devicePixelRatio = typeof window === "undefined" ? 1 : window.devicePixelRatio || 1;
  return {
    frame_id: frameId,
    generation,
    status: "error",
    message,
    viewport: {
      width: Math.max(0, Math.round(viewportElement?.clientWidth ?? 0)),
      height: Math.max(0, Math.round(viewportElement?.clientHeight ?? 0)),
      device_pixel_ratio: devicePixelRatio,
    },
    document: {
      root_empty: true,
      body_text_preview: "",
      element_count: 0,
    },
    diagnostics: [
      {
        level: "error",
        source: "runtime",
        message: `Canvas 预览构建失败：${message}`,
      },
    ],
  };
}
